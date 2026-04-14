use crate::types::{ImageEntry, RetentionRule};

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

/// Simple glob match: supports `*` (any sequence of chars) and `?` (any single char).
/// Matching is case-insensitive.
fn glob_match(pattern: &str, name: &str) -> bool {
    let pattern = pattern.to_lowercase();
    let name    = name.to_lowercase();
    glob_match_inner(pattern.as_bytes(), name.as_bytes())
}

fn glob_match_inner(mut pat: &[u8], mut txt: &[u8]) -> bool {
    let mut star_pat = &[][..];
    let mut star_txt = txt;
    loop {
        match pat.split_first() {
            None => return txt.is_empty(),
            Some((&b'*', rest)) => {
                pat      = rest;
                star_pat = rest;
                star_txt = txt;
            }
            Some((&p, rest)) => match txt.split_first() {
                None => {
                    // Consume trailing stars
                    return pat.iter().all(|&c| c == b'*');
                }
                Some((&t, trest)) => {
                    if p == b'?' || p == t {
                        pat = rest;
                        txt = trest;
                    } else if !star_pat.is_empty() {
                        // Backtrack: advance txt by one and retry from last *
                        if star_txt.is_empty() { return false; }
                        star_txt = &star_txt[1..];
                        txt      = star_txt;
                        pat      = star_pat;
                    } else {
                        return false;
                    }
                }
            },
        }
    }
}

/// Best date string for a given entry: prefers EXIF `date_taken`, falls back to `modified`.
fn entry_date(e: &ImageEntry) -> &str {
    e.date_taken.as_deref().unwrap_or(&e.modified)
}

/// Mark the single "original" in a group according to `rule`.
///
/// Canonical-name filtering is applied first for all rules: files whose name
/// contains "copy"/"copia" or doesn't match the IMG_ pattern are excluded from
/// candidacy whenever at least one canonical-named file exists in the group.
/// This preserves the prior behaviour when it does not conflict with the rule.
pub(crate) fn mark_original(entries: &mut Vec<ImageEntry>, rule: &RetentionRule) {
    // Reset previous marking
    for e in entries.iter_mut() { e.is_original = false; }

    let has_canonical = entries.iter().any(|e| !is_non_original_filename(&e.path));

    // Candidate indices: canonical-named files (or all if none are canonical).
    let candidates: Vec<usize> = entries.iter().enumerate()
        .filter(|(_, e)| !has_canonical || !is_non_original_filename(&e.path))
        .map(|(i, _)| i)
        .collect();

    if candidates.is_empty() { return; }

    let best = match rule {
        RetentionRule::HighestResolution => {
            candidates.iter().copied()
                .max_by_key(|&i| {
                    let e = &entries[i];
                    (e.size_bytes, e.width as u64 * e.height as u64)
                })
        }

        RetentionRule::OldestDate => {
            candidates.iter().copied()
                .min_by(|&a, &b| entry_date(&entries[a]).cmp(entry_date(&entries[b])))
        }

        RetentionRule::NewestDate => {
            candidates.iter().copied()
                .max_by(|&a, &b| entry_date(&entries[a]).cmp(entry_date(&entries[b])))
        }

        RetentionRule::FilenamePattern { pattern } => {
            // First try: find a candidate whose filename (stem) matches the pattern.
            let stem_of = |i: usize| -> &str {
                std::path::Path::new(&entries[i].path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
            };
            let pattern_match = candidates.iter().copied()
                .find(|&i| glob_match(pattern, stem_of(i)));

            // Fallback: highest resolution among candidates.
            pattern_match.or_else(|| {
                candidates.iter().copied()
                    .max_by_key(|&i| {
                        let e = &entries[i];
                        (e.size_bytes, e.width as u64 * e.height as u64)
                    })
            })
        }

        RetentionRule::HighestSharpness => {
            // Prefer candidates with a known blur_score; fall back to resolution.
            let with_score: Vec<usize> = candidates.iter().copied()
                .filter(|&i| entries[i].blur_score.is_some())
                .collect();

            if with_score.is_empty() {
                // No sharpness data — fall back to highest resolution.
                candidates.iter().copied()
                    .max_by_key(|&i| {
                        let e = &entries[i];
                        (e.size_bytes, e.width as u64 * e.height as u64)
                    })
            } else {
                with_score.iter().copied()
                    .max_by(|&a, &b| {
                        entries[a].blur_score.unwrap()
                            .partial_cmp(&entries[b].blur_score.unwrap())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
            }
        }
    };

    if let Some(idx) = best {
        entries[idx].is_original = true;
    }
}

pub(super) fn sort_by_date(entries: &mut Vec<ImageEntry>) {
    entries.sort_by(|a, b| a.modified.cmp(&b.modified));
}
