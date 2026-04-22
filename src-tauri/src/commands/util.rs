use std::path::PathBuf;
use base64::Engine;

/// Converts a vector of string paths to PathBuf for command processing.
pub fn to_pathbuf_vec(paths: &[String]) -> Vec<PathBuf> {
    paths.iter().map(PathBuf::from).collect()
}

/// Encodes bytes as a data URI with the given MIME type for inline display.
pub fn to_base64_data_uri(bytes: &[u8], mime_type: &str) -> String {
    format!(
        "data:{};base64,{}",
        mime_type,
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}
