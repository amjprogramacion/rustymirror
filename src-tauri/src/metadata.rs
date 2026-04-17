//! EXIF metadata read/write via the bundled ExifTool sidecar.
//!
//! All format-specific complexity (JPEG APP1 injection, ISOBMFF HEIC in-place
//! patching, little-exif for PNG/WebP) is replaced by a single `exiftool`
//! subprocess call.  ExifTool handles every format natively and atomically
//! via its own `-overwrite_original` flag.

use std::path::Path;
use anyhow::{anyhow, Result};

use crate::exiftool::find_exiftool;
use crate::types::{ImageMetadata, MetadataUpdate};

// Tags to request for a full metadata read.
// Fields that need raw numeric values use the `#` suffix (bypasses PrintConv).
// All other fields use the human-readable PrintConv form.
const READ_TAGS: &[&str] = &[
    // Dates — returned as "YYYY:MM:DD HH:MM:SS"
    // Group-qualified so HEIC files use the embedded EXIF value (local wall-clock
    // time) instead of the QuickTime atom (UTC), which would show 2 h behind for
    // a UTC+2 user because the frontend intentionally skips timezone conversion.
    "-EXIF:DateTimeOriginal",
    "-ModifyDate",
    // Camera / lens
    "-Make",
    "-Model",
    "-Software",
    "-LensMake",
    "-LensModel",
    // Exposure — human-readable strings (e.g. "1/100", "f/1.8", "28.0 mm")
    "-ExposureTime",
    "-FNumber",
    "-FocalLength",
    "-Flash",
    "-WhiteBalance",
    "-ExposureMode",
    "-ExposureProgram",
    "-MeteringMode",
    // Editable text
    "-ImageDescription",
    "-Artist",
    "-Copyright",
    // GPS — # forces decimal degrees instead of DMS string
    "-GPSLatitude#",
    "-GPSLongitude#",
    "-GPSAltitude#",
    // Dimensions / file info — # forces raw integers
    "-ImageWidth#",
    "-ImageHeight#",
    "-ExifImageWidth#",
    "-ExifImageHeight#",
    "-Orientation#",
    "-ISO#",
    "-FileSize#",
    "-FileTypeExtension",
];

// ── helpers ──────────────────────────────────────────────────────────────────

fn tag_str(obj: &serde_json::Value, key: &str) -> Option<String> {
    obj.get(key).and_then(|v| match v {
        serde_json::Value::String(s) => {
            let s = s.trim();
            if s.is_empty() { None } else { Some(s.to_owned()) }
        }
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

fn tag_f64(obj: &serde_json::Value, key: &str) -> Option<f64> {
    obj.get(key).and_then(|v| {
        v.as_f64()
            .or_else(|| v.as_str().and_then(|s| s.split_whitespace().next()?.parse().ok()))
    })
}

fn tag_u64(obj: &serde_json::Value, key: &str) -> Option<u64> {
    obj.get(key).and_then(|v| {
        v.as_u64()
            .or_else(|| v.as_str().and_then(|s| s.split_whitespace().next()?.parse().ok()))
    })
}

/// "2023:06:15 14:30:00" → "2023-06-15T14:30:00"
pub(crate) fn exif_date_to_iso(s: String) -> String {
    if s.len() < 19 {
        return s;
    }
    let b = s.as_bytes();
    if b[4] == b':' && b[7] == b':' && b[10] == b' ' {
        return format!("{}-{}-{}T{}", &s[0..4], &s[5..7], &s[8..10], &s[11..]);
    }
    s
}

/// "2023-06-15T14:30:00[Z]" → "2023:06:15 14:30:00"
fn iso_to_exif_date(s: &str) -> String {
    let s = s.trim_end_matches('Z');
    if s.len() >= 19 && s.as_bytes()[4] == b'-' {
        return format!(
            "{}:{}:{} {}",
            &s[0..4],
            &s[5..7],
            &s[8..10],
            &s[11..19]
        );
    }
    s.to_string()
}

// ── public API ────────────────────────────────────────────────────────────────

/// Read all available EXIF/file metadata from `path` using ExifTool.
pub fn read_metadata(path: &Path, resource_dir: &Path) -> Result<ImageMetadata> {
    let exiftool =
        find_exiftool(resource_dir).ok_or_else(|| anyhow!("ExifTool not found"))?;

    let obj = crate::exiftool::read_tags(&exiftool, path, READ_TAGS)?;

    // File size: exiftool returns bytes with the `#` override
    let file_size = tag_u64(&obj, "FileSize").unwrap_or_else(|| {
        std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
    });

    // Dimensions — ImageWidth/Height (File group, always present for any valid
    // image) is tried first; ExifImageWidth/Height (EXIF PixelXDimension tag,
    // optional) is a fallback for formats where the File group value can differ.
    let mut width = tag_u64(&obj, "ImageWidth")
        .or_else(|| tag_u64(&obj, "ExifImageWidth"))
        .unwrap_or(0) as u32;
    let mut height = tag_u64(&obj, "ImageHeight")
        .or_else(|| tag_u64(&obj, "ExifImageHeight"))
        .unwrap_or(0) as u32;

    let is_heic_format = path.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_lowercase().as_str(), "heic" | "heif" | "avif"))
        .unwrap_or(false);

    // For HEIC/HEIF/AVIF use the shared helper: converts to a small temp JPEG,
    // reads original dimensions, and parses DateTimeOriginal from the embedded
    // EXIF block (local wall-clock time, not QuickTime UTC).
    let mut date_time_original = tag_str(&obj, "DateTimeOriginal").map(exif_date_to_iso);
    if is_heic_format {
        let (w, h, date) = crate::heic::heic_capture_info(path, Some(resource_dir));
        if w > 0 { width = w; height = h; }
        date_time_original = date;
    }

    let format = tag_str(&obj, "FileTypeExtension")
        .map(|s| s.to_uppercase())
        .unwrap_or_else(|| {
            path.extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_uppercase())
                .unwrap_or_default()
        });

    Ok(ImageMetadata {
        file_size,
        width,
        height,
        format,
        make: tag_str(&obj, "Make"),
        model: tag_str(&obj, "Model"),
        software: tag_str(&obj, "Software"),
        date_time_original,
        date_time: tag_str(&obj, "ModifyDate").map(exif_date_to_iso),
        exposure_time: tag_str(&obj, "ExposureTime"),
        f_number: tag_str(&obj, "FNumber"),
        iso_speed: tag_u64(&obj, "ISO").map(|v| v as u32),
        focal_length: tag_str(&obj, "FocalLength"),
        flash: tag_str(&obj, "Flash"),
        white_balance: tag_str(&obj, "WhiteBalance"),
        exposure_mode: tag_str(&obj, "ExposureMode"),
        exposure_program: tag_str(&obj, "ExposureProgram"),
        metering_mode: tag_str(&obj, "MeteringMode"),
        lens_make: tag_str(&obj, "LensMake"),
        lens_model: tag_str(&obj, "LensModel"),
        gps_latitude: tag_f64(&obj, "GPSLatitude"),
        gps_longitude: tag_f64(&obj, "GPSLongitude"),
        gps_altitude: tag_f64(&obj, "GPSAltitude"),
        image_description: tag_str(&obj, "ImageDescription"),
        artist: tag_str(&obj, "Artist"),
        copyright: tag_str(&obj, "Copyright"),
        orientation: tag_u64(&obj, "Orientation").map(|v| v as u16),
    })
}

/// Write editable EXIF fields back to `path` using ExifTool.
///
/// ExifTool handles JPEG, HEIC/HEIF/AVIF, TIFF, PNG, and WebP natively.
/// `-overwrite_original` avoids leaving `_original` backup files.
pub fn write_metadata(path: &Path, update: &MetadataUpdate, resource_dir: &Path) -> Result<()> {
    let exiftool =
        find_exiftool(resource_dir).ok_or_else(|| anyhow!("ExifTool not found"))?;

    let mut tags: Vec<(&str, String)> = Vec::new();

    if let Some(ref dt) = update.date_time_original {
        let exif_dt = iso_to_exif_date(dt);
        tags.push(("DateTimeOriginal", exif_dt.clone()));
        // CreateDate is the EXIF tag used by many tools (including Apple) for
        // HEIC/MP4; keep both in sync so viewers agree on the capture time.
        tags.push(("CreateDate", exif_dt));
    }

    if let Some(ref desc) = update.image_description {
        tags.push(("ImageDescription", desc.clone()));
        tags.push(("XMP:Description", desc.clone()));
    }

    if let Some(ref a) = update.artist {
        tags.push(("Artist", a.clone()));
        tags.push(("XMP:Creator", a.clone()));
    }

    if let Some(ref c) = update.copyright {
        tags.push(("Copyright", c.clone()));
        tags.push(("XMP:Rights", c.clone()));
    }

    if let Some(lat) = update.gps_latitude {
        // ExifTool accepts signed decimal and sets Ref automatically.
        tags.push(("GPSLatitude", format!("{lat:.8}")));
        tags.push((
            "GPSLatitudeRef",
            if lat >= 0.0 { "N".into() } else { "S".into() },
        ));
    }

    if let Some(lon) = update.gps_longitude {
        tags.push(("GPSLongitude", format!("{lon:.8}")));
        tags.push((
            "GPSLongitudeRef",
            if lon >= 0.0 { "E".into() } else { "W".into() },
        ));
    }

    crate::exiftool::write_tags(&exiftool, path, &tags)
}
