//! Photo/video organizer.
//!
//! Translates the Node.js metadata-renamer CLI to Rust:
//! - Date extraction from filename patterns and EXIF metadata
//! - Rename to `{IMG|VID}_YYYYMMDD_HHMMSS_XXXX.ext`
//! - Optionally move to `{output_dir}/REORDENADAS/{year}/{device}/{MM - MONTH}/`
//! - Metadata rewrite: set DateTimeOriginal/CreateDate/DateTimeDigitized from best date

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use chrono::{Datelike, Timelike};
use rand::Rng;
use serde::{Deserialize, Serialize};

// ─── Extensions ───────────────────────────────────────────────────────────────

const IMAGE_EXTS: &[&str] = &[
    "jpg", "jpeg", "png", "heic", "heif", "avif", "cr2", "dng",
    "tif", "tiff", "webp", "bmp", "gif",
];
const VIDEO_EXTS: &[&str] = &["mp4", "mov", "avi", "mpg", "mpeg", "mkv"];

const MONTHS_ES: [&str; 12] = [
    "ENERO", "FEBRERO", "MARZO", "ABRIL", "MAYO", "JUNIO",
    "JULIO", "AGOSTO", "SEPTIEMBRE", "OCTUBRE", "NOVIEMBRE", "DICIEMBRE",
];

// ─── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum DatePriority {
    Filename,
    Exif,
}

impl Default for DatePriority {
    fn default() -> Self { DatePriority::Exif }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizerConfig {
    pub only_rename: bool,
    pub date_priority: DatePriority,
    pub override_year: bool,
    pub year_if_not_date: i32,
    pub output_directory: String,
    #[serde(default = "default_rename_template")]
    pub rename_template: String,
    #[serde(default = "default_folder_template")]
    pub folder_template: String,
}

fn default_rename_template() -> String {
    String::from("{type}_{date}_{time}_{4hex_uid}")
}

fn default_folder_template() -> String {
    String::from("REORDENADAS/{year}/{device}/{month_dir}")
}

impl Default for OrganizerConfig {
    fn default() -> Self {
        OrganizerConfig {
            only_rename: false,
            date_priority: DatePriority::Exif,
            override_year: false,
            year_if_not_date: 2015,
            output_directory: String::new(),
            rename_template: default_rename_template(),
            folder_template: default_folder_template(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DateSource {
    Filename,
    Exif,
    Modify,
    Fallback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    Image,
    Video,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizerFileAction {
    pub original_path: String,
    pub new_filename: String,
    pub new_path: String,
    pub date_used: String,
    pub date_source: DateSource,
    pub device: String,
    pub file_kind: FileKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizerProgress {
    pub processed: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizerSummary {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub failed_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewriteDateAction {
    pub path:        String,
    pub filename:    String,
    pub date:        String,
    pub date_source: DateSource,
}

// ─── File collection ──────────────────────────────────────────────────────────

pub fn collect_all_files(directories: &[PathBuf]) -> Vec<(PathBuf, FileKind)> {
    collect_all_files_internal(directories, true)
}

fn collect_all_files_internal(directories: &[PathBuf], sort: bool) -> Vec<(PathBuf, FileKind)> {
    let mut files = Vec::new();
    for dir in directories {
        walk_dir(dir, &mut files);
    }
    if sort {
        files.sort_by(|a, b| a.0.cmp(&b.0));
    }
    files
}

/// Collects files from paths, preserving input order.
/// For each path: if it's a file, add it directly; if it's a directory, enumerate its contents.
/// Used by organizer to respect frontend-provided sort order.
fn collect_files_in_order(paths: &[PathBuf]) -> Vec<(PathBuf, FileKind)> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let el = ext.to_lowercase();
                if IMAGE_EXTS.contains(&el.as_str()) {
                    files.push((path.clone(), FileKind::Image));
                } else if VIDEO_EXTS.contains(&el.as_str()) {
                    files.push((path.clone(), FileKind::Video));
                }
            }
        } else if path.is_dir() {
            walk_dir(path, &mut files);
        }
    }
    files
}

fn walk_dir(dir: &Path, out: &mut Vec<(PathBuf, FileKind)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_dir(&path, out);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let el = ext.to_lowercase();
            if IMAGE_EXTS.contains(&el.as_str()) {
                out.push((path, FileKind::Image));
            } else if VIDEO_EXTS.contains(&el.as_str()) {
                out.push((path, FileKind::Video));
            }
        }
    }
}

// ─── EXIF batch read ──────────────────────────────────────────────────────────

const ORGANIZER_TAGS: &[&str] = &[
    "-EXIF:DateTimeOriginal",
    "-EXIF:CreateDate",
    "-QuickTime:MediaCreateDate",
    "-File:FileModifyDate",
    "-Make",
    "-Model",
    "-ProductName",
    "-AndroidModel",
];

const CHUNK: usize = 500;

fn batch_exif(exiftool: &Path, paths: &[PathBuf]) -> HashMap<String, serde_json::Value> {
    match crate::exiftool::batch_read_tags(exiftool, paths, ORGANIZER_TAGS) {
        Ok(results) => results
            .into_iter()
            .filter_map(|obj| {
                let src = obj.get("SourceFile")?.as_str()?.replace('\\', "/");
                Some((src, obj))
            })
            .collect(),
        Err(e) => {
            tracing::warn!("organizer exiftool batch failed: {e}");
            HashMap::new()
        }
    }
}

fn build_exif_map(
    exiftool: &Path,
    files: &[(PathBuf, FileKind)],
    stop: &AtomicBool,
) -> HashMap<String, serde_json::Value> {
    let paths: Vec<PathBuf> = files.iter().map(|(p, _)| p.clone()).collect();
    let mut map = HashMap::new();
    for chunk in paths.chunks(CHUNK) {
        if stop.load(Ordering::Relaxed) { break; }
        map.extend(batch_exif(exiftool, chunk));
    }
    map
}

// ─── Date parsing ─────────────────────────────────────────────────────────────

pub fn parse_exif_date(s: &str) -> Option<chrono::NaiveDateTime> {
    let s = s.trim();
    if s.is_empty() || s.starts_with("0000") { return None; }

    chrono::NaiveDateTime::parse_from_str(s, "%Y:%m:%d %H:%M:%S")
        .ok()
        .or_else(|| {
            let clean = s.split(['+', 'Z']).next()?.trim();
            chrono::NaiveDateTime::parse_from_str(clean, "%Y:%m:%d %H:%M:%S").ok()
        })
        .or_else(|| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok())
        .filter(|dt| { let y = dt.year(); y >= 1900 && y <= 2100 })
}

fn fmt_exif(dt: &chrono::NaiveDateTime) -> String {
    dt.format("%Y:%m:%d %H:%M:%S").to_string()
}

fn exif_oldest_date(obj: &serde_json::Value) -> Option<String> {
    let tags = ["DateTimeOriginal", "CreateDate", "MediaCreateDate"];
    let mut dates: Vec<chrono::NaiveDateTime> = tags.iter()
        .filter_map(|t| obj.get(t)?.as_str())
        .filter_map(parse_exif_date)
        .collect();
    dates.sort();
    dates.first().map(fmt_exif)
}

// ─── Filename pattern helpers ─────────────────────────────────────────────────

fn is_digits(b: &[u8], start: usize, len: usize) -> bool {
    b.len() >= start + len && b[start..start + len].iter().all(|c| c.is_ascii_digit())
}

/// Finds `YYYY-MM-DD-HH-MM-SS` anywhere in the filename.
fn find_dashed_datetime(filename: &str) -> Option<String> {
    let b = filename.as_bytes();
    'outer: for i in 0..b.len().saturating_sub(18) {
        let lens: [usize; 6] = [4, 2, 2, 2, 2, 2];
        let mut pos = i;
        let mut parts = [""; 6];
        for (gi, &len) in lens.iter().enumerate() {
            if !is_digits(b, pos, len) { continue 'outer; }
            parts[gi] = &filename[pos..pos + len];
            pos += len;
            if gi < 5 {
                if b.get(pos) != Some(&b'-') { continue 'outer; }
                pos += 1;
            }
        }
        let candidate = format!(
            "{}:{}:{} {}:{}:{}",
            parts[0], parts[1], parts[2], parts[3], parts[4], parts[5]
        );
        if parse_exif_date(&candidate).is_some() {
            return Some(candidate);
        }
    }
    None
}

/// Finds `YYYYMMDD_HHMMSS` anywhere in the filename.
fn find_compact_datetime(filename: &str) -> Option<String> {
    let b = filename.as_bytes();
    for i in 0..b.len().saturating_sub(14) {
        if !is_digits(b, i, 8) { continue; }
        if b.get(i + 8) != Some(&b'_') { continue; }
        if !is_digits(b, i + 9, 6) { continue; }
        let candidate = format!(
            "{}:{}:{} {}:{}:{}",
            &filename[i..i + 4], &filename[i + 4..i + 6], &filename[i + 6..i + 8],
            &filename[i + 9..i + 11], &filename[i + 11..i + 13], &filename[i + 13..i + 15],
        );
        if parse_exif_date(&candidate).is_some() {
            return Some(candidate);
        }
    }
    None
}

/// Finds `YYYYMMDD_HHMM` (no seconds) — secondary pattern.
fn find_compact_datetime_no_sec(filename: &str) -> Option<String> {
    let b = filename.as_bytes();
    for i in 0..b.len().saturating_sub(12) {
        if !is_digits(b, i, 8) { continue; }
        if b.get(i + 8) != Some(&b'_') { continue; }
        if !is_digits(b, i + 9, 4) { continue; }
        // Skip if actually YYYYMMDD_HHMMSS (6 digits after underscore)
        if b.len() >= i + 15 && b[i + 13].is_ascii_digit() && b[i + 14].is_ascii_digit() {
            continue;
        }
        let candidate = format!(
            "{}:{}:{} {}:{}:00",
            &filename[i..i + 4], &filename[i + 4..i + 6], &filename[i + 6..i + 8],
            &filename[i + 9..i + 11], &filename[i + 11..i + 13],
        );
        if parse_exif_date(&candidate).is_some() {
            return Some(candidate);
        }
    }
    None
}

/// Finds `IMG_YYYYMMDD_` pattern (case-insensitive) — returns "YYYY:MM:DD".
fn find_img_date_key(filename: &str) -> Option<String> {
    let upper = filename.to_uppercase();
    let idx = upper.find("IMG_")?;
    let after = &upper[idx + 4..];
    let b = after.as_bytes();
    if !is_digits(b, 0, 8) { return None; }
    if b.get(8) != Some(&b'_') { return None; }
    Some(format!("{}:{}:{}", &after[..4], &after[4..6], &after[6..8]))
}

/// Finds `-YYYYMMDD-WA` pattern — returns "YYYY:MM:DD".
fn find_wa_date_key(filename: &str) -> Option<String> {
    let upper = filename.to_uppercase();
    let b = upper.as_bytes();
    for i in 0..b.len().saturating_sub(11) {
        if b[i] != b'-' { continue; }
        if !is_digits(b, i + 1, 8) { continue; }
        if b.get(i + 9) != Some(&b'-') { continue; }
        if !upper[i + 10..].starts_with("WA") { continue; }
        let orig = &filename[i + 1..i + 9];
        return Some(format!("{}:{}:{}", &orig[..4], &orig[4..6], &orig[6..8]));
    }
    None
}

fn incremental_time(date_key: &str, cache: &mut HashMap<String, u32>) -> String {
    let m = {
        let entry = cache.entry(date_key.to_string()).or_insert(0);
        let v = *entry;
        *entry = (v + 1) % 1440;
        v
    };
    format!("{} {:02}:{:02}:00", date_key, m / 60, m % 60)
}

fn apply_year_override(date: &str, config: &OrganizerConfig) -> String {
    if config.override_year && date.starts_with("2025") {
        format!("{:04}{}", config.year_if_not_date, &date[4..])
    } else {
        date.to_string()
    }
}

/// Returns the best date extractable from the filename alone (primary patterns).
pub fn filename_date(filename: &str) -> Option<String> {
    find_dashed_datetime(filename)
        .or_else(|| find_compact_datetime(filename))
        .or_else(|| find_compact_datetime_no_sec(filename))
}

// ─── Public: date/device extraction ──────────────────────────────────────────

pub fn extract_date(
    filename: &str,
    exif_obj: &serde_json::Value,
    config: &OrganizerConfig,
    incr_cache: &mut HashMap<String, u32>,
) -> (String, DateSource) {
    let from_filename = find_dashed_datetime(filename)
        .or_else(|| find_compact_datetime(filename));
    let from_exif = exif_oldest_date(exif_obj);
    let from_modify = exif_obj.get("FileModifyDate").and_then(|v| v.as_str()).and_then(|d| {
        let clean = &d[..d.len().min(19)];
        chrono::NaiveDateTime::parse_from_str(clean, "%Y:%m:%d %H:%M:%S")
            .ok()
            .map(|p| p.format("%Y:%m:%d %H:%M:%S").to_string())
    });

    let (date, source) = match config.date_priority {
        DatePriority::Filename => {
            if let Some(d) = from_filename         { (Some(d), DateSource::Filename) }
            else if let Some(d) = from_exif        { (Some(d), DateSource::Exif) }
            else if let Some(d) = from_modify      { (Some(d), DateSource::Modify) }
            else                                   { (None, DateSource::Fallback) }
        }
        DatePriority::Exif => {
            if let Some(d) = from_exif             { (Some(d), DateSource::Exif) }
            else if let Some(d) = from_modify      { (Some(d), DateSource::Modify) }
            else if let Some(d) = from_filename    { (Some(d), DateSource::Filename) }
            else                                   { (None, DateSource::Fallback) }
        }
    };

    if let Some(d) = date {
        return (apply_year_override(&d, config), source);
    }

    // Secondary filename patterns (used only when primary + EXIF both fail)
    if let Some(d) = find_compact_datetime_no_sec(filename) {
        return (apply_year_override(&d, config), DateSource::Filename);
    }
    if let Some(key) = find_img_date_key(filename) {
        let full = incremental_time(&key, incr_cache);
        return (apply_year_override(&full, config), DateSource::Filename);
    }
    if let Some(key) = find_wa_date_key(filename) {
        let full = incremental_time(&key, incr_cache);
        return (apply_year_override(&full, config), DateSource::Filename);
    }

    let now = chrono::Local::now().naive_local();
    let fallback = format!(
        "{:04}:{:02}:{:02} {:02}:{:02}:{:02}",
        config.year_if_not_date,
        now.month(), now.day(),
        now.hour(), now.minute(), now.second(),
    );
    (fallback, DateSource::Fallback)
}

pub fn extract_device(filename: &str, exif_obj: &serde_json::Value) -> String {
    for tag in &["Model", "ProductName", "AndroidModel"] {
        if let Some(v) = exif_obj.get(tag).and_then(|v| v.as_str()) {
            let v = v.trim();
            if !v.is_empty() { return v.to_uppercase(); }
        }
    }
    let upper = filename.to_uppercase();
    if upper.contains("WA-") || upper.contains("-WA") {
        return "WHATSAPP".to_string();
    }
    "DESCONOCIDO".to_string()
}

// ─── Filename / path building ─────────────────────────────────────────────────

static ID_COUNTER: AtomicU32 = AtomicU32::new(0);

fn generate_hex_uid(len: usize) -> String {
    let n = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:0>width$X}", n, width = len)
}

fn normalise_ext(ext: &str) -> &str {
    if ext.eq_ignore_ascii_case("jpeg") { "jpg" } else { ext }
}

const CRYPTO_UID_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

fn generate_crypto_uid(len: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| CRYPTO_UID_CHARS[rng.gen_range(0..CRYPTO_UID_CHARS.len())] as char)
        .collect()
}

fn apply_hex_uid_tokens(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let start = i;
            i += 1;
            let digit_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
            let digit_end = i;
            if s[i..].starts_with("hex_uid}") {
                let n = if digit_end > digit_start {
                    s[digit_start..digit_end].parse().unwrap_or(4).max(1).min(32)
                } else {
                    4
                };
                result.push_str(&generate_hex_uid(n));
                i += "hex_uid}".len();
                continue;
            }
            result.push_str(&s[start..i]);
            continue;
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

fn apply_crypto_uid_tokens(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Look for pattern {Ncrypto_uid} where N is one or more digits
        if bytes[i] == b'{' {
            let start = i;
            i += 1;
            let digit_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
            let digit_end = i;
            if digit_end > digit_start && s[i..].starts_with("crypto_uid}") {
                let n: usize = s[digit_start..digit_end].parse().unwrap_or(4).max(1).min(64);
                result.push_str(&generate_crypto_uid(n));
                i += "crypto_uid}".len();
                continue;
            }
            // Not a match — emit the characters as-is
            result.push_str(&s[start..i]);
            continue;
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

fn build_new_filename(kind: &FileKind, date: &str, ext: &str, config: &OrganizerConfig) -> Option<String> {
    if date.len() < 19 { return None; }
    let year  = &date[..4];
    let month = &date[5..7];
    let day   = &date[8..10];
    let hour  = &date[11..13];
    let min   = &date[14..16];
    let sec   = &date[17..19];
    let date_part = format!("{}{}{}", year, month, day);
    let time_part = format!("{}{}{}", hour, min, sec);
    let type_str  = match kind { FileKind::Image => "IMG", FileKind::Video => "VID" };

    let tpl = if config.rename_template.is_empty() {
        "{type}_{date}_{time}_{4hex_uid}"
    } else {
        &config.rename_template
    };

    let stem = tpl
        .replace("{type}",   type_str)
        .replace("{date}",   &date_part)
        .replace("{time}",   &time_part)
        .replace("{year}",   year)
        .replace("{month}",  month)
        .replace("{day}",    day)
        .replace("{hour}",   hour)
        .replace("{min}",    min)
        .replace("{sec}",    sec);

    let stem = apply_hex_uid_tokens(&stem);
    let stem = apply_crypto_uid_tokens(&stem);

    Some(format!("{}.{}", stem, normalise_ext(ext)))
}

fn build_target_dir(config: &OrganizerConfig, date: &str, device: &str) -> Option<PathBuf> {
    if config.output_directory.is_empty() || date.len() < 10 { return None; }
    let year  = &date[..4];
    let day   = &date[8..10];
    let month_num: usize = date[5..7].parse().ok()?;
    if month_num < 1 || month_num > 12 { return None; }
    let month_name = MONTHS_ES[month_num - 1];
    let month_str  = format!("{:02}", month_num);
    let month_dir  = format!("{} - {}", month_str, month_name);

    let device_clean: String = device.chars()
        .map(|c| if "\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect();

    let tpl = if config.folder_template.is_empty() {
        "REORDENADAS/{year}/{device}/{month_dir}"
    } else {
        &config.folder_template
    };

    let resolved = tpl
        .replace("{year}",       year)
        .replace("{month}",      &month_str)
        .replace("{month_name}", month_name)
        .replace("{month_dir}",  &month_dir)
        .replace("{device}",     &device_clean)
        .replace("{day}",        day);

    let mut path = PathBuf::from(&config.output_directory);
    for segment in resolved.split('/') {
        let s = segment.trim();
        if !s.is_empty() { path = path.join(s); }
    }
    Some(path)
}

fn move_file(src: &Path, dst: &Path) -> std::io::Result<()> {
    match std::fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(_) => {
            // Cross-device move: copy then delete
            std::fs::copy(src, dst)?;
            std::fs::remove_file(src)
        }
    }
}

fn try_rename_file(
    path: &Path,
    kind: &FileKind,
    date: &str,
    device: &str,
    ext: &str,
    config: &OrganizerConfig,
) -> std::io::Result<()> {
    let current_dir = path.parent().unwrap_or(Path::new(""));
    for _ in 0..16 {
        let filename = build_new_filename(kind, date, ext, config)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "invalid date string"))?;
        let dest = if config.only_rename {
            current_dir.join(&filename)
        } else {
            let dir = build_target_dir(config, date, device).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::Other, "output_directory not configured")
            })?;
            std::fs::create_dir_all(&dir)?;
            dir.join(&filename)
        };
        if dest.exists() { continue; }
        return move_file(path, &dest);
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not resolve filename collision after 16 attempts",
    ))
}

// ─── Public operations ────────────────────────────────────────────────────────

fn process_files_with_order(
    files: &[(PathBuf, FileKind)],
    config: &OrganizerConfig,
    exif_map: &HashMap<String, serde_json::Value>,
    stop: &Arc<AtomicBool>,
    on_progress: &impl Fn(OrganizerProgress),
) -> Vec<OrganizerFileAction> {
    ID_COUNTER.store(0, Ordering::Relaxed);
    let total = files.len();
    let mut actions = Vec::with_capacity(total);
    let mut incr: HashMap<String, u32> = HashMap::new();
    let mut reserved: std::collections::HashSet<String> = std::collections::HashSet::new();

    let empty = serde_json::Value::Object(Default::default());

    for (i, (path, kind)) in files.iter().enumerate() {
        if stop.load(Ordering::Relaxed) { break; }
        let path_str = path.to_string_lossy().to_string();
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let exif_obj = exif_map.get(&path_str.replace('\\', "/")).unwrap_or(&empty);

        let (date, date_source) = extract_date(filename, exif_obj, config, &mut incr);
        let device = extract_device(filename, exif_obj);
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

        let found = (0..16).find_map(|_| {
            let new_filename = build_new_filename(kind, &date, &ext, config)?;
            let new_path = if config.only_rename {
                path.parent().unwrap_or(Path::new("")).join(&new_filename)
                    .to_string_lossy().to_string()
            } else {
                build_target_dir(config, &date, &device)
                    .map(|d| d.join(&new_filename).to_string_lossy().to_string())
                    .unwrap_or_else(|| new_filename.clone())
            };
            if Path::new(&new_path).exists() || reserved.contains(&new_path) {
                return None;
            }
            Some((new_filename, new_path))
        });

        let Some((new_filename, new_path)) = found else { continue };
        reserved.insert(new_path.clone());

        actions.push(OrganizerFileAction {
            original_path: path_str,
            new_filename,
            new_path,
            date_used: date,
            date_source,
            device,
            file_kind: kind.clone(),
        });
        on_progress(OrganizerProgress { processed: i + 1, total });
    }
    actions
}

#[allow(dead_code)]
pub fn preview(
    directories: &[PathBuf],
    config: &OrganizerConfig,
    exiftool: Option<&Path>,
    stop: Arc<AtomicBool>,
    on_progress: impl Fn(OrganizerProgress),
) -> Vec<OrganizerFileAction> {
    let files = collect_all_files(directories);
    let exif_map = exiftool
        .map(|et| build_exif_map(et, &files, &stop))
        .unwrap_or_default();
    process_files_with_order(&files, config, &exif_map, &stop, &on_progress)
}

/// Preview with files in their provided order (respects UI sorting).
/// Treats input paths as either files (used directly) or directories (enumerated).
/// Preserves the exact order provided from the frontend.
pub fn preview_files_ordered(
    paths: &[PathBuf],
    config: &OrganizerConfig,
    exiftool: Option<&Path>,
    stop: Arc<AtomicBool>,
    on_progress: impl Fn(OrganizerProgress),
) -> Vec<OrganizerFileAction> {
    let files = collect_files_in_order(paths);
    let exif_map = exiftool
        .map(|et| build_exif_map(et, &files, &stop))
        .unwrap_or_default();

    process_files_with_order(&files, config, &exif_map, &stop, &on_progress)
}

pub fn execute(
    directories: &[PathBuf],
    config: &OrganizerConfig,
    exiftool: Option<&Path>,
    stop: Arc<AtomicBool>,
    on_progress: impl Fn(OrganizerProgress),
) -> OrganizerSummary {
    ID_COUNTER.store(0, Ordering::Relaxed);
    let files = collect_all_files(directories);
    let total = files.len();
    let mut succeeded = 0usize;
    let mut failed_paths: Vec<String> = Vec::new();
    let mut incr: HashMap<String, u32> = HashMap::new();

    let exif_map = exiftool
        .map(|et| build_exif_map(et, &files, &stop))
        .unwrap_or_default();

    let empty = serde_json::Value::Object(Default::default());

    for (i, (path, kind)) in files.iter().enumerate() {
        if stop.load(Ordering::Relaxed) { break; }
        let path_str = path.to_string_lossy().to_string();
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let exif_obj = exif_map.get(&path_str.replace('\\', "/")).unwrap_or(&empty);

        let (date, _) = extract_date(filename, exif_obj, config, &mut incr);
        let device = extract_device(filename, exif_obj);
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

        match try_rename_file(path, kind, &date, &device, &ext, config) {
            Ok(()) => succeeded += 1,
            Err(e) => {
                tracing::warn!("organize failed for {}: {}", path_str, e);
                failed_paths.push(path_str);
            }
        }
        on_progress(OrganizerProgress { processed: i + 1, total });
    }
    OrganizerSummary { total, succeeded, failed: failed_paths.len(), failed_paths }
}

pub fn rewrite_metadata(
    directories: &[PathBuf],
    config: &OrganizerConfig,
    exiftool: Option<&Path>,
    stop: Arc<AtomicBool>,
    on_progress: impl Fn(OrganizerProgress),
) -> OrganizerSummary {
    let Some(exiftool) = exiftool else {
        return OrganizerSummary {
            total: 0, succeeded: 0, failed: 1,
            failed_paths: vec!["ExifTool not available".to_string()],
        };
    };

    let files = collect_all_files(directories);
    let total = files.len();
    let mut succeeded = 0usize;
    let mut failed_paths: Vec<String> = Vec::new();
    let mut incr: HashMap<String, u32> = HashMap::new();

    let exif_map = build_exif_map(exiftool, &files, &stop);
    let empty = serde_json::Value::Object(Default::default());

    for (i, (path, _)) in files.iter().enumerate() {
        if stop.load(Ordering::Relaxed) { break; }
        let path_str = path.to_string_lossy().to_string();
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let exif_obj = exif_map.get(&path_str.replace('\\', "/")).unwrap_or(&empty);

        let (date, _) = extract_date(filename, exif_obj, config, &mut incr);
        let tags: Vec<(&str, String)> = vec![
            ("DateTimeOriginal",  date.clone()),
            ("CreateDate",        date.clone()),
            ("DateTimeDigitized", date.clone()),
        ];

        match crate::exiftool::write_tags(exiftool, path, &tags) {
            Ok(()) => succeeded += 1,
            Err(e) => {
                tracing::warn!("metadata rewrite failed for {}: {}", path_str, e);
                failed_paths.push(path_str);
            }
        }
        on_progress(OrganizerProgress { processed: i + 1, total });
    }
    OrganizerSummary { total, succeeded, failed: failed_paths.len(), failed_paths }
}

/// Preview metadata rewrite with files in their provided order (respects UI sorting).
/// Does NOT re-sort alphabetically — preserves the order from the frontend.
pub fn preview_rewrite_metadata_ordered(
    paths: &[PathBuf],
    config: &OrganizerConfig,
    exiftool: Option<&Path>,
    stop: Arc<AtomicBool>,
    on_progress: impl Fn(OrganizerProgress),
) -> Vec<RewriteDateAction> {
    let files = collect_files_in_order(paths);
    let total = files.len();
    let mut actions = Vec::with_capacity(total);
    let mut incr: HashMap<String, u32> = HashMap::new();

    let exif_map = exiftool
        .map(|et| build_exif_map(et, &files, &stop))
        .unwrap_or_default();
    let empty = serde_json::Value::Object(Default::default());

    for (i, (path, _)) in files.iter().enumerate() {
        if stop.load(Ordering::Relaxed) { break; }
        let path_str = path.to_string_lossy().to_string();
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        let exif_obj = exif_map.get(&path_str.replace('\\', "/")).unwrap_or(&empty);

        let (date, date_source) = extract_date(&filename, exif_obj, config, &mut incr);
        actions.push(RewriteDateAction { path: path_str, filename, date, date_source });
        on_progress(OrganizerProgress { processed: i + 1, total });
    }
    actions
}
