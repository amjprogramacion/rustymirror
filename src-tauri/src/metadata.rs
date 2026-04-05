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
        other          => other,
    }.to_string();

    // Read image dimensions via image crate
    let (width, height) = image::image_dimensions(path).unwrap_or((0, 0));

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

    // Helper: get a string field from EXIF display value (strips surrounding quotes)
    let str_field = |tag: exif::Tag| -> Option<String> {
        exif.get_field(tag, exif::In::PRIMARY).map(|f| {
            let s = f.display_value().to_string();
            // kamadak-exif wraps ASCII strings in quotes — strip them
            let s = s.trim_matches('"').trim().to_string();
            if s.is_empty() { return String::new() }
            s
        }).filter(|s| !s.is_empty())
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

/// Writes editable EXIF fields back to the file using little-exif.
/// Only JPEG and TIFF files are supported; others return an error.
pub fn write_metadata(path: &Path, update: &MetadataUpdate) -> Result<()> {
    use little_exif::metadata::Metadata;
    use little_exif::exif_tag::ExifTag;

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "jpg" | "jpeg" | "tiff" | "tif" | "png" | "webp" => {}
        other => return Err(anyhow!("Metadata writing not supported for .{}", other)),
    }

    let mut metadata = Metadata::new_from_path(path)
        .map_err(|e| anyhow!("Failed to read metadata: {e:?}"))?;

    if let Some(ref dt) = update.date_time_original {
        // Convert ISO 8601 → EXIF date format "YYYY:MM:DD HH:MM:SS"
        let exif_dt = iso_to_exif_date(dt);
        metadata.set_tag(ExifTag::DateTimeOriginal(exif_dt));
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

    metadata
        .write_to_file(path)
        .map_err(|e| anyhow!("Failed to write metadata: {e:?}"))?;

    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// "2023:06:15 14:30:00" → "2023-06-15T14:30:00"
fn exif_date_to_iso(s: String) -> String {
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
fn parse_gps_coord(
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
