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

/// Selects the best available date by iterating the caller-supplied priority order.
/// Returns (date, source) tuple where source identifies the date origin.
pub fn select_date_by_priority(
    priority_order: &[crate::organizer::DateSourceOrder],
    from_filename: Option<String>,
    from_exif: Option<String>,
    from_exif_source: &str,
    from_modify: Option<String>,
) -> Option<(String, String)> {
    use crate::organizer::DateSourceOrder;
    for slot in priority_order {
        match slot {
            DateSourceOrder::Filename => {
                if let Some(d) = from_filename.clone() { return Some((d, "filename".to_owned())); }
            }
            DateSourceOrder::Exif => {
                if let Some(d) = from_exif.clone() { return Some((d, from_exif_source.to_owned())); }
            }
            DateSourceOrder::Modify => {
                if let Some(d) = from_modify.clone() { return Some((d, "modify".to_owned())); }
            }
        }
    }
    None
}

/// Same as `process_exif_chunk` but uses an already-running `ExifToolDaemon`
/// instead of spawning a new process per call.
pub fn process_exif_chunk_daemon(
    daemon: &mut crate::exiftool::ExifToolDaemon,
    chunk: &[&PathBuf],
    priority_order: &[crate::organizer::DateSourceOrder],
) -> std::collections::HashMap<String, (String, String)> {
    use std::collections::HashMap;

    let mut date_map: HashMap<String, (String, String)> = HashMap::new();

    let owned: Vec<PathBuf> = chunk.iter().map(|p| (*p).clone()).collect();
    let results = match daemon.batch_query(
        &owned,
        &["-EXIF:DateTimeOriginal", "-QuickTime:CreateDate", "-File:FileModifyDate"],
    ) {
        Ok(r) => r,
        Err(_) => return date_map,
    };

    for obj in results {
        let src_path = obj.get("SourceFile")
            .and_then(|v| v.as_str())
            .map(|s| s.replace('\\', "/"));

        let filename = src_path.as_deref()
            .and_then(|p| p.rsplit('/').next())
            .unwrap_or("");

        let from_filename = crate::organizer::filename_date(filename);
        let from_exif = obj.get("DateTimeOriginal")
            .or_else(|| obj.get("CreateDate"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned());
        let from_exif_source = if obj.get("DateTimeOriginal").and_then(|v| v.as_str()).is_some() {
            "exif"
        } else {
            "create"
        };
        let from_modify = obj.get("FileModifyDate")
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned());

        if let (Some(src), Some(entry)) = (src_path, select_date_by_priority(
            priority_order,
            from_filename,
            from_exif,
            from_exif_source,
            from_modify,
        )) {
            date_map.insert(src, entry);
        }
    }

    date_map
}

/// Processes a chunk of file paths via ExifTool, extracting date metadata.
/// Returns a map of normalized path → (date, source).
pub fn process_exif_chunk(
    et: &PathBuf,
    chunk: &[&PathBuf],
    priority_order: &[crate::organizer::DateSourceOrder],
) -> std::collections::HashMap<String, (String, String)> {
    use std::collections::HashMap;

    let mut date_map: HashMap<String, (String, String)> = HashMap::new();

    // Convert references to owned PathBuf for the batch operation
    let owned: Vec<PathBuf> = chunk.iter().map(|p| (*p).clone()).collect();

    // Query EXIF dates and file modification time
    let results = match crate::exiftool::batch_read_tags(
        et,
        &owned,
        &["-EXIF:DateTimeOriginal", "-QuickTime:CreateDate", "-File:FileModifyDate"],
    ) {
        Ok(r) => r,
        Err(_) => return date_map, // Return empty map on error, don't fail the whole operation
    };

    for obj in results {
        let src_path = obj.get("SourceFile")
            .and_then(|v| v.as_str())
            .map(|s| s.replace('\\', "/"));

        let filename = src_path.as_deref()
            .and_then(|p| p.rsplit('/').next())
            .unwrap_or("");

        let from_filename = crate::organizer::filename_date(filename);
        let from_exif = obj.get("DateTimeOriginal")
            .or_else(|| obj.get("CreateDate"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned());
        let from_exif_source = if obj.get("DateTimeOriginal").and_then(|v| v.as_str()).is_some() {
            "exif"
        } else {
            "create"
        };
        let from_modify = obj.get("FileModifyDate")
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned());

        if let (Some(src), Some(entry)) = (src_path, select_date_by_priority(
            priority_order,
            from_filename,
            from_exif,
            from_exif_source,
            from_modify,
        )) {
            date_map.insert(src, entry);
        }
    }

    date_map
}
