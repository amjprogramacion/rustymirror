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
