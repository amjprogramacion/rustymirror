use std::path::Path;
use anyhow::{anyhow, Result};

use crate::types::{ImageMetadata, MetadataUpdate};

/// Reads all available EXIF and file metadata from an image file.
pub fn read_metadata(path: &Path) -> Result<ImageMetadata> {
    let bytes = std::fs::read(path)?;
    let file_size = bytes.len() as u64;

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let format = match ext.as_str() {
        "jpg" | "jpeg" => "JPEG",
        "png"          => "PNG",
        "webp"         => "WebP",
        "bmp"          => "BMP",
        "gif"          => "GIF",
        "tiff" | "tif" => "TIFF",
        "heic" | "heif"=> "HEIC",
        "avif"         => "AVIF",
        other          => other,
    }.to_string();

    // Read image dimensions. HEIC/AVIF cannot be decoded by the image crate,
    // so we delegate to magick identify (same tool used for pHash conversion).
    let (width, height) = if matches!(ext.as_str(), "heic" | "heif" | "avif") {
        crate::heic::heic_dimensions(path, None)
    } else {
        image::image_dimensions(path).unwrap_or((0, 0))
    };

    // Parse EXIF
    let exif_result = exif::Reader::new()
        .read_from_container(&mut std::io::Cursor::new(&bytes));

    let mut meta = ImageMetadata {
        file_size,
        width,
        height,
        format,
        make: None,
        model: None,
        software: None,
        date_time_original: None,
        date_time: None,
        exposure_time: None,
        f_number: None,
        iso_speed: None,
        focal_length: None,
        flash: None,
        white_balance: None,
        exposure_mode: None,
        exposure_program: None,
        metering_mode: None,
        lens_make: None,
        lens_model: None,
        gps_latitude: None,
        gps_longitude: None,
        gps_altitude: None,
        image_description: None,
        artist: None,
        copyright: None,
        orientation: None,
    };

    let Ok(exif) = exif_result else { return Ok(meta) };

    // Helper: get a string field from EXIF.
    // Reads Value::Ascii directly (first non-empty component) to avoid display_value()
    // concatenating multiple components that some camera firmware writes into a single tag.
    let str_field = |tag: exif::Tag| -> Option<String> {
        exif.get_field(tag, exif::In::PRIMARY).and_then(|f| {
            match &f.value {
                exif::Value::Ascii(parts) => parts
                    .iter()
                    .filter_map(|p| std::str::from_utf8(p).ok())
                    .map(|s| s.trim_matches('\0').trim().to_string())
                    .find(|s| !s.is_empty()),
                _ => {
                    let s = f.display_value().to_string();
                    let s = s.trim_matches('"').trim().to_string();
                    if s.is_empty() { None } else { Some(s) }
                }
            }
        })
    };

    meta.make     = str_field(exif::Tag::Make);
    meta.model    = str_field(exif::Tag::Model);
    meta.software = str_field(exif::Tag::Software);
    meta.image_description = str_field(exif::Tag::ImageDescription);
    meta.artist    = str_field(exif::Tag::Artist);
    meta.copyright = str_field(exif::Tag::Copyright);

    // Dates — convert "YYYY:MM:DD HH:MM:SS" → ISO 8601
    meta.date_time_original = str_field(exif::Tag::DateTimeOriginal).map(exif_date_to_iso);
    meta.date_time          = str_field(exif::Tag::DateTime).map(exif_date_to_iso);

    // Orientation
    if let Some(f) = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY) {
        if let exif::Value::Short(ref v) = f.value {
            meta.orientation = v.first().copied();
        }
    }

    // Exposure
    meta.exposure_time  = str_field(exif::Tag::ExposureTime);
    meta.f_number       = str_field(exif::Tag::FNumber);
    meta.focal_length   = str_field(exif::Tag::FocalLength);
    meta.flash          = str_field(exif::Tag::Flash);
    meta.white_balance  = str_field(exif::Tag::WhiteBalance);
    meta.exposure_mode  = str_field(exif::Tag::ExposureMode);
    meta.exposure_program = str_field(exif::Tag::ExposureProgram);
    meta.metering_mode  = str_field(exif::Tag::MeteringMode);

    // ISO — stored as Short
    if let Some(f) = exif.get_field(exif::Tag::PhotographicSensitivity, exif::In::PRIMARY) {
        if let exif::Value::Short(ref v) = f.value {
            meta.iso_speed = v.first().map(|&x| x as u32);
        }
    }

    // Lens
    meta.lens_make  = str_field(exif::Tag::LensMake);
    meta.lens_model = str_field(exif::Tag::LensModel);

    // GPS
    meta.gps_latitude  = parse_gps_coord(&exif, exif::Tag::GPSLatitude,  exif::Tag::GPSLatitudeRef);
    meta.gps_longitude = parse_gps_coord(&exif, exif::Tag::GPSLongitude, exif::Tag::GPSLongitudeRef);

    if let Some(f) = exif.get_field(exif::Tag::GPSAltitude, exif::In::PRIMARY) {
        if let exif::Value::Rational(ref v) = f.value {
            if let Some(r) = v.first() { meta.gps_altitude = Some(r.to_f64()); }
        }
    }

    Ok(meta)
}

/// Writes editable EXIF fields back to the file.
/// JPEG uses kamadak-exif writer (supports GPS + preserves existing fields).
/// HEIC/HEIF/AVIF use an in-place ISOBMFF EXIF replacement.
/// TIFF/PNG/WebP fall back to little-exif (no GPS support for those formats).
pub fn write_metadata(path: &Path, update: &MetadataUpdate) -> Result<()> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // Detect real format by magic bytes — a recovered file may have the wrong
    // extension (e.g. a JPEG saved with a .heic name).
    let actual_format = detect_format_by_magic(path).unwrap_or_else(|| ext.clone());

    match actual_format.as_str() {
        "jpg" | "jpeg" => write_metadata_jpeg(path, update),
        "heic" | "heif" | "avif" => write_metadata_heic(path, update),
        "tiff" | "tif" | "png" | "webp" => write_metadata_little_exif(path, update),
        _ => match ext.as_str() {
            "jpg" | "jpeg" => write_metadata_jpeg(path, update),
            "heic" | "heif" | "avif" => write_metadata_heic(path, update),
            "tiff" | "tif" | "png" | "webp" => write_metadata_little_exif(path, update),
            other => Err(anyhow!("Metadata writing not supported for .{}", other)),
        },
    }
}

/// Reads the first few bytes of a file and returns the actual format name if
/// the magic bytes are recognised, or `None` if unknown / unreadable.
fn detect_format_by_magic(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut buf = [0u8; 12];
    let n = std::fs::File::open(path).ok()?.read(&mut buf).ok()?;
    let buf = &buf[..n];
    if buf.starts_with(b"\xff\xd8\xff") {
        Some("jpeg".into())
    } else if buf.starts_with(b"\x89PNG") {
        Some("png".into())
    } else if buf.starts_with(b"RIFF") && buf.get(8..12) == Some(b"WEBP") {
        Some("webp".into())
    } else if buf.starts_with(b"II") || buf.starts_with(b"MM") {
        Some("tiff".into())
    } else {
        None // HEIC/AVIF/unknown — let extension routing handle it
    }
}

/// Applies the `MetadataUpdate` fields onto an existing `field_map` built from
/// `exif::parse_exif`.  Each `Some` field in `update` overwrites or inserts the
/// corresponding EXIF tag; `None` fields are left untouched.
fn apply_metadata_updates(
    field_map: &mut std::collections::HashMap<(exif::Tag, exif::In), exif::Field>,
    update: &MetadataUpdate,
) {
    use exif::{Field, In, Tag, Value};

    if let Some(ref dt) = update.date_time_original {
        let exif_dt = iso_to_exif_date(dt);
        field_map.insert(
            (Tag::DateTimeOriginal, In::PRIMARY),
            Field { tag: Tag::DateTimeOriginal, ifd_num: In::PRIMARY, value: Value::Ascii(vec![exif_dt.into_bytes()]) },
        );
    }
    if let Some(ref desc) = update.image_description {
        field_map.insert(
            (Tag::ImageDescription, In::PRIMARY),
            Field { tag: Tag::ImageDescription, ifd_num: In::PRIMARY, value: Value::Ascii(vec![desc.as_bytes().to_vec()]) },
        );
    }
    if let Some(ref artist) = update.artist {
        field_map.insert(
            (Tag::Artist, In::PRIMARY),
            Field { tag: Tag::Artist, ifd_num: In::PRIMARY, value: Value::Ascii(vec![artist.as_bytes().to_vec()]) },
        );
    }
    if let Some(ref copy) = update.copyright {
        field_map.insert(
            (Tag::Copyright, In::PRIMARY),
            Field { tag: Tag::Copyright, ifd_num: In::PRIMARY, value: Value::Ascii(vec![copy.as_bytes().to_vec()]) },
        );
    }
    if let (Some(lat), Some(lon)) = (update.gps_latitude, update.gps_longitude) {
        let lat_ref = if lat >= 0.0 { b"N".to_vec() } else { b"S".to_vec() };
        let lon_ref = if lon >= 0.0 { b"E".to_vec() } else { b"W".to_vec() };
        let lat_dms = decimal_to_dms(lat.abs());
        let lon_dms = decimal_to_dms(lon.abs());
        field_map.insert(
            (Tag::GPSLatitudeRef, In::PRIMARY),
            Field { tag: Tag::GPSLatitudeRef, ifd_num: In::PRIMARY, value: Value::Ascii(vec![lat_ref]) },
        );
        field_map.insert(
            (Tag::GPSLatitude, In::PRIMARY),
            Field { tag: Tag::GPSLatitude, ifd_num: In::PRIMARY, value: Value::Rational(lat_dms.to_vec()) },
        );
        field_map.insert(
            (Tag::GPSLongitudeRef, In::PRIMARY),
            Field { tag: Tag::GPSLongitudeRef, ifd_num: In::PRIMARY, value: Value::Ascii(vec![lon_ref]) },
        );
        field_map.insert(
            (Tag::GPSLongitude, In::PRIMARY),
            Field { tag: Tag::GPSLongitude, ifd_num: In::PRIMARY, value: Value::Rational(lon_dms.to_vec()) },
        );
    }
}

/// JPEG path: read all existing EXIF fields, apply updates (including GPS),
/// re-encode with kamadak-exif Writer, and inject the new APP1 segment.
fn write_metadata_jpeg(path: &Path, update: &MetadataUpdate) -> Result<()> {
    use exif::{experimental::Writer, Field, In};
    use std::collections::HashMap;
    use std::io::BufReader;

    let bytes = std::fs::read(path)?;

    // Collect existing EXIF fields so we preserve camera data, orientation, etc.
    let mut field_map: HashMap<(exif::Tag, In), Field> = HashMap::new();
    if let Ok(exif_bytes) =
        exif::get_exif_attr_from_jpeg(&mut BufReader::new(std::io::Cursor::new(&bytes)))
    {
        if let Ok((fields, _le)) = exif::parse_exif(&exif_bytes) {
            for f in fields {
                field_map.insert((f.tag, f.ifd_num), f);
            }
        }
    }

    // Apply updates — each Some field overwrites (or inserts) the corresponding entry.
    apply_metadata_updates(&mut field_map, update);

    // Encode to TIFF bytes
    let fields: Vec<Field> = field_map.into_values().collect();
    let mut writer = Writer::new();
    for f in &fields {
        writer.push_field(f);
    }
    let mut buf = std::io::Cursor::new(Vec::<u8>::new());
    writer
        .write(&mut buf, true) // little-endian
        .map_err(|e| anyhow!("EXIF encode error: {e}"))?;
    let tiff_bytes = buf.into_inner();

    // Inject the new APP1 segment into the JPEG stream and write atomically
    let new_jpeg = replace_app1_in_jpeg(&bytes, &tiff_bytes)?;
    write_atomic(path, &new_jpeg)
}

/// HEIC/HEIF/AVIF path: locate the EXIF item in the ISOBMFF container via the
/// `iloc` box, update the TIFF/EXIF payload in-place (zero-padded to the
/// original slot size), and write atomically.
///
/// Requires that the file already contains an EXIF item and that the updated
/// EXIF fits within the existing slot.  Camera-produced files satisfy both
/// conditions in practice.
fn write_metadata_heic(path: &Path, update: &MetadataUpdate) -> Result<()> {
    use exif::{experimental::Writer, Field, In, Value};
    use std::collections::HashMap;

    let mut file_bytes = std::fs::read(path)?;

    // Locate the EXIF item payload (offset + length) in the ISOBMFF container.
    // This must succeed before we attempt any field parsing.
    let (item_offset, item_length) = crate::heic::find_exif_in_heic(&file_bytes)?;
    if item_length < 4 {
        return Err(anyhow!("HEIC EXIF item too small ({item_length} B)"));
    }

    // Build the field map from the existing EXIF using kamadak-exif's own
    // read_from_container, which correctly handles:
    //   • tiff_header_offset (the 4-byte prefix that precedes the TIFF data)
    //   • both big- and little-endian TIFF
    //   • ExifIFD / GPS IFD sub-IFD resolution
    // This is the same path used by read_metadata, so it is reliable.
    //
    // Fallback: if read_from_container fails (e.g. AVIF or non-standard brand),
    // parse the TIFF bytes manually after respecting tiff_header_offset.
    let mut field_map: HashMap<(exif::Tag, In), Field> = HashMap::new();

    let exif_reader_ok = if let Ok(exif) = exif::Reader::new()
        .read_from_container(&mut std::io::Cursor::new(&file_bytes))
    {
        for f in exif.fields() {
            // Only preserve primary-image fields (ifd_num 0).  Thumbnail IFDs
            // are not part of the HEIC EXIF item and would confuse the writer.
            // Skip Value::Unknown — the Writer cannot serialise those and would
            // return an error; proprietary/unknown tags are a minor data loss
            // acceptable in exchange for a successful write.
            if f.ifd_num == In::PRIMARY && !matches!(f.value, Value::Unknown(..)) {
                field_map.insert((f.tag, f.ifd_num), f.clone());
            }
        }
        true
    } else {
        false
    };

    if !exif_reader_ok {
        // Manual fallback: skip the 4-byte tiff_header_offset prefix, then
        // additionally skip `header_off` more bytes as the spec allows.
        let header_off = u32::from_be_bytes(
            file_bytes[item_offset..item_offset + 4].try_into().unwrap()
        ) as usize;
        let tiff_start = item_offset + 4 + header_off;
        let tiff_len   = item_length.saturating_sub(4 + header_off);
        if tiff_start + tiff_len <= file_bytes.len() {
            if let Ok((fields, _)) =
                exif::parse_exif(&file_bytes[tiff_start..tiff_start + tiff_len])
            {
                for f in fields {
                    if f.ifd_num == In::PRIMARY && !matches!(f.value, Value::Unknown(..)) {
                        field_map.insert((f.tag, f.ifd_num), f);
                    }
                }
            }
        }
    }

    // Apply updates.
    apply_metadata_updates(&mut field_map, update);

    // Encode new EXIF (TIFF bytes).
    let fields: Vec<Field> = field_map.into_values().collect();
    let mut writer = Writer::new();
    for f in &fields {
        writer.push_field(f);
    }
    let mut buf = std::io::Cursor::new(Vec::<u8>::new());
    writer
        .write(&mut buf, false) // big-endian — matches the byte order read_from_container finds
        .map_err(|e| anyhow!("EXIF encode error: {e}"))?;
    let new_tiff_bytes = buf.into_inner();

    // Total new item size: 4-byte prefix (tiff_header_offset=0) + TIFF payload.
    let new_item_size = 4 + new_tiff_bytes.len();
    if new_item_size > item_length {
        return Err(anyhow!(
            "Updated EXIF ({} B) exceeds the existing EXIF slot ({} B) in the HEIC \
             container — in-place update not possible",
            new_item_size, item_length
        ));
    }

    // Overwrite in-place:
    //   bytes[item_offset..+4]   = 0x00000000 (tiff_header_offset = 0)
    //   bytes[item_offset+4..+N] = new TIFF bytes
    //   remainder                = zero padding
    file_bytes[item_offset..item_offset + 4].copy_from_slice(&[0, 0, 0, 0]);
    let payload_end = item_offset + 4 + new_tiff_bytes.len();
    file_bytes[item_offset + 4..payload_end].copy_from_slice(&new_tiff_bytes);
    for b in &mut file_bytes[payload_end..item_offset + item_length] {
        *b = 0;
    }

    write_atomic(path, &file_bytes)
}

/// Non-JPEG path: write basic fields using little-exif (GPS not supported).
fn write_metadata_little_exif(path: &Path, update: &MetadataUpdate) -> Result<()> {
    use little_exif::metadata::Metadata;
    use little_exif::exif_tag::ExifTag;

    let mut metadata = Metadata::new_from_path(path)
        .map_err(|e| anyhow!("Failed to read metadata: {e:?}"))?;

    if let Some(ref dt) = update.date_time_original {
        metadata.set_tag(ExifTag::DateTimeOriginal(iso_to_exif_date(dt)));
    }
    if let Some(ref desc) = update.image_description {
        metadata.set_tag(ExifTag::ImageDescription(desc.clone()));
    }
    if let Some(ref artist) = update.artist {
        metadata.set_tag(ExifTag::Artist(artist.clone()));
    }
    if let Some(ref copy) = update.copyright {
        metadata.set_tag(ExifTag::Copyright(copy.clone()));
    }

    // little_exif writes in-place, so we write atomically by:
    // 1. copying the original to a .tmp sibling (same filesystem → rename is cheap),
    // 2. injecting EXIF into the temp copy,
    // 3. renaming into place.
    // The temp file is removed on any failure.
    let mut tmp_name = path.as_os_str().to_owned();
    tmp_name.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp_name);

    let result = (|| -> Result<()> {
        std::fs::copy(path, &tmp)
            .map_err(|e| anyhow!("Failed to create temp file: {e}"))?;
        metadata
            .write_to_file(&tmp)
            .map_err(|e| anyhow!("Failed to write metadata: {e:?}"))?;
        std::fs::rename(&tmp, path)
            .map_err(|e| anyhow!("Failed to rename temp file: {e}"))
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Decimal degrees → [degrees, minutes, seconds] as EXIF Rational triples.
fn decimal_to_dms(decimal: f64) -> [exif::Rational; 3] {
    use exif::Rational;
    let degrees = decimal.floor() as u32;
    let minutes_f = (decimal - degrees as f64) * 60.0;
    let minutes = minutes_f.floor() as u32;
    let seconds_num = ((minutes_f - minutes as f64) * 60.0 * 10_000.0).round() as u32;
    [
        Rational { num: degrees,     denom: 1 },
        Rational { num: minutes,     denom: 1 },
        Rational { num: seconds_num, denom: 10_000 },
    ]
}

/// Write `data` to `path` atomically:
/// 1. Write to `{path}.tmp` in the same directory (same filesystem → rename is a rename, not a copy).
/// 2. Rename into place — the original is replaced only once the new data is fully on disk.
/// 3. Remove the temp file on any failure so no `.tmp` orphan is left behind.
fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    let mut tmp_name = path.as_os_str().to_owned();
    tmp_name.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp_name);

    if let Err(e) = std::fs::write(&tmp, data) {
        let _ = std::fs::remove_file(&tmp);
        return Err(anyhow!("Failed to write temp file: {e}"));
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        anyhow!("Failed to rename temp file: {e}")
    })
}

/// Replace (or insert) the EXIF APP1 segment in a JPEG byte stream.
fn replace_app1_in_jpeg(jpeg: &[u8], tiff_bytes: &[u8]) -> Result<Vec<u8>> {
    if jpeg.len() < 2 || jpeg[0] != 0xFF || jpeg[1] != 0xD8 {
        return Err(anyhow!("Not a valid JPEG file"));
    }

    let exif_header: &[u8] = b"Exif\0\0";
    let data_len = exif_header.len() + tiff_bytes.len(); // bytes after marker
    let seg_len = (data_len + 2) as u16;                 // length field includes itself

    let mut new_app1 = Vec::with_capacity(4 + data_len);
    new_app1.extend_from_slice(&[0xFF, 0xE1]);
    new_app1.extend_from_slice(&seg_len.to_be_bytes());
    new_app1.extend_from_slice(exif_header);
    new_app1.extend_from_slice(tiff_bytes);

    let mut result = Vec::with_capacity(jpeg.len() + new_app1.len());
    result.extend_from_slice(&[0xFF, 0xD8]); // SOI

    let mut pos = 2;
    let mut replaced = false;

    while pos + 1 < jpeg.len() {
        if jpeg[pos] != 0xFF {
            result.extend_from_slice(&jpeg[pos..]);
            break;
        }
        let marker = jpeg[pos + 1];

        // Stand-alone markers (no length field): RST0-RST7, SOI, EOI
        if marker == 0xD8 || marker == 0xD9 || (0xD0..=0xD7).contains(&marker) {
            if marker == 0xD9 && !replaced {
                result.extend_from_slice(&new_app1);
                replaced = true;
            }
            result.push(0xFF);
            result.push(marker);
            pos += 2;
            continue;
        }

        if pos + 4 > jpeg.len() {
            result.extend_from_slice(&jpeg[pos..]);
            break;
        }

        let seg_bytes = u16::from_be_bytes([jpeg[pos + 2], jpeg[pos + 3]]) as usize;
        if seg_bytes < 2 || pos + 2 + seg_bytes > jpeg.len() {
            result.extend_from_slice(&jpeg[pos..]);
            break;
        }
        let seg_end = pos + 2 + seg_bytes;

        // SOS: scan data follows — emit everything from here to the end
        if marker == 0xDA {
            if !replaced {
                result.extend_from_slice(&new_app1);
                replaced = true;
            }
            result.extend_from_slice(&jpeg[pos..]);
            break;
        }

        // APP1 with EXIF header → replace
        let data_start = pos + 4;
        if marker == 0xE1
            && seg_bytes >= 8
            && data_start + 6 <= jpeg.len()
            && &jpeg[data_start..data_start + 6] == b"Exif\0\0"
        {
            if !replaced {
                result.extend_from_slice(&new_app1);
                replaced = true;
            }
            // Skip old APP1
            pos = seg_end;
            continue;
        }

        // Keep all other segments as-is
        result.extend_from_slice(&jpeg[pos..seg_end]);
        pos = seg_end;
    }

    if !replaced {
        // No APP1 found — insert immediately after SOI
        let mut out = Vec::with_capacity(2 + new_app1.len() + jpeg.len() - 2);
        out.extend_from_slice(&[0xFF, 0xD8]);
        out.extend_from_slice(&new_app1);
        out.extend_from_slice(&jpeg[2..]);
        return Ok(out);
    }

    Ok(result)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// "2023:06:15 14:30:00" → "2023-06-15T14:30:00"
pub(crate) fn exif_date_to_iso(s: String) -> String {
    if s.len() < 19 { return s; }
    let b = s.as_bytes();
    // Expect format YYYY:MM:DD HH:MM:SS
    if b[4] == b':' && b[7] == b':' && b[10] == b' ' {
        return format!(
            "{}-{}-{}T{}",
            &s[0..4], &s[5..7], &s[8..10], &s[11..]
        );
    }
    s
}

/// "2023-06-15T14:30:00" → "2023:06:15 14:30:00"
fn iso_to_exif_date(s: &str) -> String {
    // Handle both "2023-06-15T14:30:00" and "2023-06-15T14:30:00Z"
    let s = s.trim_end_matches('Z');
    if s.len() >= 19 && s.as_bytes()[4] == b'-' {
        return format!(
            "{}:{}:{} {}",
            &s[0..4], &s[5..7], &s[8..10], &s[11..19]
        );
    }
    s.to_string()
}

/// Parse GPS coordinate from DMS rationals + reference direction.
pub(crate) fn parse_gps_coord(
    exif: &exif::Exif,
    coord_tag: exif::Tag,
    ref_tag: exif::Tag,
) -> Option<f64> {
    let coord = exif.get_field(coord_tag, exif::In::PRIMARY)?;
    let ref_f = exif.get_field(ref_tag,   exif::In::PRIMARY)?;

    let decimal = if let exif::Value::Rational(ref v) = coord.value {
        if v.len() < 3 { return None; }
        v[0].to_f64() + v[1].to_f64() / 60.0 + v[2].to_f64() / 3600.0
    } else {
        return None;
    };

    let direction = ref_f.display_value().to_string();
    let direction = direction.trim_matches('"');
    if direction == "S" || direction == "W" {
        Some(-decimal)
    } else {
        Some(decimal)
    }
}
