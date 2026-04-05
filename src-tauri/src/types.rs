use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SimilarityKind {
    Exact,
    Similar,
    /// Same capture date+time but different content (e.g. burst, edited copy)
    SameDate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageEntry {
    pub path: String,
    pub size_bytes: u64,
    pub width: u32,
    pub height: u32,
    /// ISO-8601 string so Vue can parse it easily
    pub modified: String,
    pub is_original: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    pub kind: SimilarityKind,
    pub entries: Vec<ImageEntry>,
    /// Similarity percentage (0-100). 100 = exact, None = not applicable (sameDate/exact)
    pub similarity: Option<u8>,
}

/// Full EXIF + file metadata for the metadata panel
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageMetadata {
    // File info
    pub file_size: u64,
    pub width: u32,
    pub height: u32,
    pub format: String,
    // Camera
    pub make: Option<String>,
    pub model: Option<String>,
    pub software: Option<String>,
    // Dates
    pub date_time_original: Option<String>,
    pub date_time: Option<String>,
    // Exposure
    pub exposure_time: Option<String>,
    pub f_number: Option<String>,
    pub iso_speed: Option<u32>,
    pub focal_length: Option<String>,
    pub flash: Option<String>,
    pub white_balance: Option<String>,
    pub exposure_mode: Option<String>,
    pub exposure_program: Option<String>,
    pub metering_mode: Option<String>,
    // Lens
    pub lens_make: Option<String>,
    pub lens_model: Option<String>,
    // GPS
    pub gps_latitude: Option<f64>,
    pub gps_longitude: Option<f64>,
    pub gps_altitude: Option<f64>,
    // Editable fields
    pub image_description: Option<String>,
    pub artist: Option<String>,
    pub copyright: Option<String>,
    pub orientation: Option<u16>,
}

/// Fields the user can modify in the metadata panel
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataUpdate {
    pub date_time_original: Option<String>,
    pub image_description: Option<String>,
    pub artist: Option<String>,
    pub copyright: Option<String>,
}

/// Emitted during phase 1 (file hashing)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub scanned: usize,
    pub total: usize,
}

/// Emitted during phase 4 (perceptual analysis)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeProgress {
    pub analyzed: usize,
    pub total: usize,
    /// Human-readable label for the current phase
    pub phase: String,
}
