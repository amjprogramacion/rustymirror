use crate::types::ImageEntry;

pub(super) fn is_non_original_filename(path: &str) -> bool {
    let stem = std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    let lower = stem.to_lowercase();

    // Rule 1: name explicitly marks it as a copy
    if lower.contains("copy") || lower.contains("copia") {
        return true;
    }

    // Rule 2: name does not follow canonical pattern IMG_YYYYMMDD_HHMMSS_XXXX
    // where XXXX is exactly 4 alphanumeric characters
    let canonical = regex_is_canonical(stem);
    !canonical
}

pub(super) fn regex_is_canonical(stem: &str) -> bool {
    // Pattern: IMG_ + 8 digits + _ + 6 digits + _ + 4 alphanumeric chars
    // Example: IMG_20210828_132922_A23C
    if !stem.starts_with("IMG_") { return false; }
    let rest = &stem[4..]; // skip "IMG_"
    // Expect: 8 digits
    if rest.len() < 8 { return false; }
    if !rest[..8].chars().all(|c| c.is_ascii_digit()) { return false; }
    let rest = &rest[8..]; // skip date
    // Expect: _ + 6 digits
    if !rest.starts_with('_') { return false; }
    let rest = &rest[1..];
    if rest.len() < 6 { return false; }
    if !rest[..6].chars().all(|c| c.is_ascii_digit()) { return false; }
    let rest = &rest[6..]; // skip time
    // Expect: _ + exactly 4 alphanumeric chars, nothing more
    if !rest.starts_with('_') { return false; }
    let rest = &rest[1..];
    rest.len() == 4 && rest.chars().all(|c| c.is_ascii_alphanumeric())
}

pub(super) fn mark_original(entries: &mut Vec<ImageEntry>) {
    let has_canonical = entries.iter().any(|e| !is_non_original_filename(&e.path));

    let candidates: Vec<usize> = entries.iter().enumerate()
        .filter(|(_, e)| !has_canonical || !is_non_original_filename(&e.path))
        .map(|(i, _)| i)
        .collect();

    if let Some(&best) = candidates.iter()
        .max_by_key(|&&i| (entries[i].size_bytes, entries[i].width as u64 * entries[i].height as u64))
    {
        entries[best].is_original = true;
    }
}

pub(super) fn sort_by_date(entries: &mut Vec<ImageEntry>) {
    entries.sort_by(|a, b| a.modified.cmp(&b.modified));
}
