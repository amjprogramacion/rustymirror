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

/// Extracts a string value from a JSON EXIF tag, trimming whitespace and skipping empty values.
pub fn extract_tag_string(obj: Option<&serde_json::Value>, key: &str) -> Option<String> {
    obj?.get(key).and_then(|v| match v {
        serde_json::Value::String(s) => {
            let s = s.trim();
            if s.is_empty() { None } else { Some(s.to_owned()) }
        }
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

/// Extracts a float value from a JSON EXIF tag, handling both numeric and string representations.
pub fn extract_tag_f64(obj: Option<&serde_json::Value>, key: &str) -> Option<f64> {
    let v = obj?.get(key)?;
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.split_whitespace().next()?.parse().ok()))
}

/// Extracts an unsigned integer value from a JSON EXIF tag, handling both numeric and string representations.
pub fn extract_tag_u64(obj: Option<&serde_json::Value>, key: &str) -> Option<u64> {
    let v = obj?.get(key)?;
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.split_whitespace().next()?.parse().ok()))
}
