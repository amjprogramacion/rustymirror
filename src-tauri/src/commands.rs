use std::path::PathBuf;
use std::sync::{Arc, Mutex};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter, Manager, State};

/// Returns the cache directory for the current build mode.
/// In debug builds, uses a `dev` subdirectory to keep dev caches separate from release caches.
fn cache_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, tauri::Error> {
    let base = app.path().app_data_dir()?;
    if cfg!(debug_assertions) {
        Ok(base.join("dev"))
    } else {
        Ok(base)
    }
}

use crate::scanner::{apply_retention_rule, find_duplicates};
use crate::types::{AnalyzeProgress, DuplicateGroup, FailedFile, FailedFileKind, RetentionRule, ScanProgress};

/// Structured error type returned by all Tauri commands.
/// Serialises as `{ "type": "...", "message": "...", "path": "..." }` so the
/// frontend can display context-rich messages and, if needed, branch on `type`.
#[derive(Debug, serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AppError {
    /// Scan or duplicate-detection failure.
    Scan { message: String },
    /// File deletion failure (network or local).
    Delete { path: String, message: String },
    /// Generic I/O or filesystem error.
    Io { message: String },
    /// EXIF metadata read/write failure.
    Metadata { path: String, message: String },
    /// Thumbnail or full-image generation failure.
    Thumbnail { message: String },
    /// Unexpected internal error (task panic, join failure, etc.).
    Internal { message: String },
}

impl From<tokio::task::JoinError> for AppError {
    fn from(e: tokio::task::JoinError) -> Self {
        AppError::Internal { message: e.to_string() }
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        AppError::Io { message: e.to_string() }
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub groups: Vec<DuplicateGroup>,
    pub failed_files: Vec<FailedFile>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaScanResult {
    pub images: Vec<crate::types::ImageEntry>,
    pub failed_files: Vec<FailedFile>,
}

/// Shared atomic stop flag — set to true when the user clicks "Stop scan".
/// Wrapped in Mutex<Arc<...>> so we can replace it for each new scan.
pub struct ScanState(pub Mutex<Arc<AtomicBool>>);

/// Same pattern for the metadata scan.
pub struct MetaScanState(pub Mutex<Arc<AtomicBool>>);

/// Caches the file list from directory_fingerprint so scan_directories can
/// reuse it without a second SMB traversal (avoids count discrepancy on NAS).
pub struct FileListCache(pub Mutex<Option<(Vec<String>, Vec<std::path::PathBuf>)>>);

#[tauri::command]
pub async fn scan_directories(
    paths: Vec<String>,
    hamming_threshold: Option<u32>,
    cross_date_phash: Option<bool>,
    fast_mode: Option<bool>,
    retention_rule: Option<RetentionRule>,
    app: AppHandle,
    scan_state: State<'_, ScanState>,
    file_list_cache: State<'_, FileListCache>,
) -> Result<ScanResult, AppError> {
    let stop = Arc::new(AtomicBool::new(false));
    *scan_state.0.lock().unwrap() = stop.clone();

    let directories: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let app_clone  = app.clone();
    let app_clone2 = app.clone();
    let resource_dir = app.path().resource_dir().ok();
    let cache = cache_data_dir(&app).ok()
        .and_then(|d| crate::cache::Cache::open(&d).ok())
        .map(std::sync::Arc::new);
    tracing::debug!("cache: {}", if cache.is_some() { "ok" } else { "unavailable" });
    tracing::debug!("scan_directories called with {} paths", paths.len());
    for p in &paths { tracing::debug!("  path: {}", p); }

    let threshold    = hamming_threshold.unwrap_or(6);
    let use_fast     = fast_mode.unwrap_or(false);
    let rule         = retention_rule.unwrap_or(RetentionRule::HighestResolution);
    tracing::debug!("hamming threshold: {}", threshold);
    tracing::debug!("fast mode: {}", use_fast);

    // Reuse the file list from directory_fingerprint if it matches the current paths,
    // to avoid a second SMB directory traversal that may return fewer files (NAS cache cold).
    let prefetched = {
        let mut guard = file_list_cache.0.lock().unwrap();
        if let Some((cached_paths, file_list)) = guard.take() {
            if cached_paths == paths {
                tracing::debug!("reusing {} prefetched paths from fingerprint", file_list.len());
                Some(file_list)
            } else {
                None
            }
        } else {
            None
        }
    };

    let result = tokio::task::spawn_blocking(move || {
        tracing::debug!("spawn_blocking started");
        let r = find_duplicates(
            directories,
            prefetched,
            resource_dir,
            stop,
            threshold,
            cache,
            cross_date_phash.unwrap_or(true),
            use_fast,
            rule,
            move |scanned, total| {
                if scanned == 1 || scanned % 50 == 0 || scanned == total {
                    tracing::debug!("progress {}/{}", scanned, total);
                }
                let _ = app_clone.emit("scan_progress", ScanProgress { scanned, total });
            },
            move |progress: AnalyzeProgress| {
                let _ = app_clone2.emit("analyze_progress", &progress);
            },
        );
        tracing::debug!("find_duplicates returned: {}", if r.is_ok() { "Ok" } else { "Err" });
        r
    })
    .await;

    tracing::debug!("spawn_blocking joined: {}", match &result { Ok(_) => "Ok", Err(_) => "JoinError" });

    let (groups, failed_files) = result
        .map_err(|e| { tracing::debug!("JoinError: {}", e); AppError::from(e) })?
        .map_err(|e| { tracing::debug!("ScanError: {}", e); AppError::Scan { message: e.to_string() } })?;
    Ok(ScanResult { groups, failed_files })
}

/// Re-apply a retention rule to an existing set of groups without a full rescan.
/// Returns the same groups with updated `is_original` flags.
#[tauri::command]
pub fn apply_retention_rule_cmd(
    groups: Vec<DuplicateGroup>,
    rule: RetentionRule,
) -> Vec<DuplicateGroup> {
    apply_retention_rule(groups, &rule)
}

#[tauri::command]
pub fn stop_scan(scan_state: State<'_, ScanState>) {
    tracing::debug!("stop requested");
    scan_state.0.lock().unwrap().store(true, Ordering::Relaxed);
}

#[tauri::command]
pub fn stop_meta_scan(meta_scan_state: State<'_, MetaScanState>) {
    tracing::debug!("meta scan stop requested");
    meta_scan_state.0.lock().unwrap().store(true, Ordering::Relaxed);
}

#[tauri::command]
pub async fn delete_files(paths: Vec<String>, app: tauri::AppHandle) -> Result<(), AppError> {
    use tauri::Emitter;
    let total = paths.len();
    tracing::debug!("delete_files: {} files", total);

    #[cfg(target_os = "windows")]
    {
        for (i, path) in paths.iter().enumerate() {
            let p = path.trim();
            if p.is_empty() { continue; }

            // Network paths (UNC) have no recycle bin — delete permanently.
            // Local paths go to the recycle bin via the trash crate.
            let is_network = p.starts_with("\\\\") || p.starts_with("//");
            if is_network {
                let meta = std::fs::metadata(p)
                    .map_err(|e| AppError::Delete { path: p.to_string(), message: e.to_string() })?;
                if meta.is_dir() {
                    std::fs::remove_dir_all(p)
                        .map_err(|e| AppError::Delete { path: p.to_string(), message: e.to_string() })?;
                } else {
                    std::fs::remove_file(p)
                        .map_err(|e| AppError::Delete { path: p.to_string(), message: e.to_string() })?;
                }
            } else {
                trash::delete(p)
                    .map_err(|e| AppError::Delete { path: p.to_string(), message: e.to_string() })?;
            }
            let _ = app.emit("delete_progress", serde_json::json!({ "done": i + 1, "total": total }));
        }
        tracing::debug!("deleted {} files", total);

        if let Ok(data_dir) = cache_data_dir(&app) {
            if let Ok(cache) = crate::cache::Cache::open(&data_dir) {
                cache.evict_deleted(&paths);
                tracing::debug!(count = paths.len(), "cache entries evicted");
            }
        }

        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    {
        for (i, path) in paths.iter().enumerate() {
            trash::delete(path)
                .map_err(|e| AppError::Delete { path: path.clone(), message: e.to_string() })?;
            let _ = app.emit("delete_progress", serde_json::json!({ "done": i + 1, "total": total }));
        }
        if let Ok(data_dir) = cache_data_dir(&app) {
            if let Ok(cache) = crate::cache::Cache::open(&data_dir) {
                cache.evict_deleted(&paths);
            }
        }
        Ok(())
    }
}

#[tauri::command]
pub fn log_message(level: String, message: String) {
    match level.as_str() {
        "error" => eprintln!("[RustyMirror:JS] ERROR — {}", message),
        "warn"  => eprintln!("[RustyMirror:JS] WARN  — {}", message),
        _       => tracing::debug!(target: "rustymirror::js", "{}", message),
    }
}

#[tauri::command]
pub async fn get_thumbnail(path: String, app: tauri::AppHandle) -> Result<String, AppError> {
    use tauri::Manager;

    let resource_dir    = app.path().resource_dir().ok();
    let thumb_cache_dir = cache_data_dir(&app).ok().map(|d| d.join("thumb_cache"));

    tokio::task::spawn_blocking(move || -> Result<String, AppError> {
        use image::imageops::FilterType;
        use std::io::{Cursor, Seek, SeekFrom};
        use base64::Engine;

        let thumb_err = |msg: String| AppError::Thumbnail { message: msg };

        let lower   = path.to_lowercase();
        let is_heic = lower.ends_with(".heic") || lower.ends_with(".heif") || lower.ends_with(".avif");

        if is_heic {
            // Read once — used for the cache key.
            let heic_bytes = std::fs::read(&path).map_err(|e| thumb_err(e.to_string()))?;

            let cache_path = thumb_cache_dir.as_ref().map(|dir| {
                let hash = blake3::hash(&heic_bytes);
                let name = format!("{}.jpg", &hash.to_hex()[..16]);
                dir.join(name)
            });

            if let Some(ref cp) = cache_path {
                if cp.exists() {
                    if let Ok(cached) = std::fs::read(cp) {
                        tracing::debug!("thumb HIT (heic): {}", path);
                        return Ok(format!("data:image/jpeg;base64,{}",
                            base64::engine::general_purpose::STANDARD.encode(&cached)));
                    }
                }
            }

            tracing::debug!("thumb MISS (heic): {}", path);

            let (tmp, _, _) = crate::heic::heic_to_temp_jpeg(
                std::path::Path::new(&path),
                resource_dir.as_deref(),
                None, // full resolution for thumbnail/viewer
            ).ok_or_else(|| thumb_err("HEIC converter not available".to_string()))?;

            let jpeg_bytes = std::fs::read(&tmp).map_err(|e| thumb_err(e.to_string()))?;
            let _ = std::fs::remove_file(&tmp);

            let img = image::load_from_memory(&jpeg_bytes).map_err(|e| thumb_err(e.to_string()))?;
            // Normalise to 8-bit RGB — mirrors the PNG fix; prevents JPEG encoder
            // failures when magick/sips produces output with an unusual bit depth
            // or colour space (e.g. HDR/wide-gamut HEICs from iPhone Pro models).
            let img   = image::DynamicImage::ImageRgb8(img.into_rgb8());
            let thumb = img.resize(180, 180, FilterType::Nearest);
            let mut buf = Cursor::new(Vec::<u8>::new());
            thumb.write_to(&mut buf, image::ImageFormat::Jpeg).map_err(|e| thumb_err(e.to_string()))?;
            buf.seek(SeekFrom::Start(0)).map_err(|e| thumb_err(e.to_string()))?;
            let thumb_bytes = buf.into_inner();

            if let Some(ref cp) = cache_path {
                if let Some(parent) = cp.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(cp, &thumb_bytes);
                tracing::debug!("thumb SAVE (heic): {}", path);
            }

            return Ok(format!("data:image/jpeg;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&thumb_bytes)));
        }

        // Non-HEIC: handles local PNGs (WebView2 struggles with some variants)
        // and network paths (which cannot use convertFileSrc).
        let bytes = std::fs::read(&path).map_err(|e| thumb_err(e.to_string()))?;

        let cache_path = thumb_cache_dir.as_ref().and_then(|dir| {
            let hash = blake3::hash(&bytes);
            let name = format!("jpg_{}.jpg", &hash.to_hex()[..16]);
            Some(dir.join(name))
        });

        if let Some(ref cp) = cache_path {
            if cp.exists() {
                if let Ok(cached) = std::fs::read(cp) {
                    tracing::debug!("thumb HIT (jpg/net): {}", path);
                    return Ok(format!("data:image/jpeg;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(&cached)));
                }
            }
        }

        tracing::debug!("thumb MISS (jpg/net): {}", path);

        let img = match image::load_from_memory(&bytes) {
            Ok(img) => img,
            Err(e) => {
                // The image crate failed to decode the file (unusual PNG variant,
                // unsupported bit depth, etc.). Return the raw bytes as a data URI
                // so the browser can still try to render it natively.
                tracing::warn!(error = %e, path = %path, "thumb decode failed, returning raw");
                let ext = std::path::Path::new(&path)
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                let mime = match ext.as_str() {
                    "png"        => "image/png",
                    "gif"        => "image/gif",
                    "webp"       => "image/webp",
                    "bmp"        => "image/bmp",
                    "tiff"|"tif" => "image/tiff",
                    _            => "image/jpeg",
                };
                return Ok(format!("data:{};base64,{}",
                    mime,
                    base64::engine::general_purpose::STANDARD.encode(&bytes)));
            }
        };
        let img   = apply_exif_orientation(&bytes, img);
        // Normalise to 8-bit RGB before resize: JPEG does not support 16-bit colour
        // depth, so 48-bit (16bpc) PNGs would cause write_to to fail otherwise.
        let img   = image::DynamicImage::ImageRgb8(img.into_rgb8());
        let thumb = img.resize(180, 180, FilterType::Nearest);
        let mut buf = Cursor::new(Vec::<u8>::new());
        thumb.write_to(&mut buf, image::ImageFormat::Jpeg).map_err(|e| thumb_err(e.to_string()))?;
        buf.seek(SeekFrom::Start(0)).map_err(|e| thumb_err(e.to_string()))?;
        let thumb_bytes = buf.into_inner();

        if let Some(ref cp) = cache_path {
            if let Some(parent) = cp.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(cp, &thumb_bytes);
            tracing::debug!("thumb SAVE (jpg/net): {}", path);
        }

        Ok(format!("data:image/jpeg;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(thumb_bytes)))
    })
    .await?
}

fn apply_exif_orientation(bytes: &[u8], img: image::DynamicImage) -> image::DynamicImage {
    let orientation = read_exif_orientation(bytes).unwrap_or(1);
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}

fn read_exif_orientation(bytes: &[u8]) -> Option<u32> {
    let mut cursor = std::io::Cursor::new(bytes);
    let exif = exif::Reader::new().read_from_container(&mut cursor).ok()?;
    let field = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)?;
    match field.value {
        exif::Value::Short(ref v) => v.first().map(|&x| x as u32),
        _ => None,
    }
}

#[tauri::command]
pub async fn get_full_image(path: String, app: tauri::AppHandle) -> Result<String, AppError> {
    use tauri::Manager;
    let resource_dir = app.path().resource_dir().ok();

    tokio::task::spawn_blocking(move || -> Result<String, AppError> {
        use std::io::{Cursor, Seek, SeekFrom};
        use base64::Engine;

        let thumb_err = |msg: String| AppError::Thumbnail { message: msg };

        let lower   = path.to_lowercase();
        let is_heic = lower.ends_with(".heic") || lower.ends_with(".heif") || lower.ends_with(".avif");

        let bytes = if is_heic {
            let (tmp, _, _) = crate::heic::heic_to_temp_jpeg(
                std::path::Path::new(&path),
                resource_dir.as_deref(),
                None, // full resolution for thumbnail/viewer
            ).ok_or_else(|| thumb_err("HEIC converter not available".to_string()))?;
            let b = std::fs::read(&tmp).map_err(|e| thumb_err(e.to_string()))?;
            let _ = std::fs::remove_file(&tmp);
            b
        } else {
            std::fs::read(&path).map_err(|e| thumb_err(e.to_string()))?
        };

        let img = image::load_from_memory(&bytes).map_err(|e| thumb_err(e.to_string()))?;
        let img = if !is_heic { apply_exif_orientation(&bytes, img) } else { img };
        // Normalise to 8-bit RGB — prevents JPEG encoder failures on HDR/wide-gamut
        // or CMYK output from ImageMagick (mirrors the same fix in get_thumbnail).
        let img = image::DynamicImage::ImageRgb8(img.into_rgb8());

        let mut buf = Cursor::new(Vec::<u8>::new());
        img.write_to(&mut buf, image::ImageFormat::Jpeg).map_err(|e| thumb_err(e.to_string()))?;
        buf.seek(SeekFrom::Start(0)).map_err(|e| thumb_err(e.to_string()))?;

        Ok(format!("data:image/jpeg;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(buf.into_inner())))
    })
    .await?
}

/// Opens a file with its default application.
/// On Windows, delegating to explorer.exe avoids the security zone warning
/// dialog that appears when launching files via ShellExecuteW from an
/// unsigned process — explorer.exe is a trusted system process.
#[tauri::command]
pub fn open_file(path: String) -> Result<(), AppError> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&path)
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .spawn()
            .map_err(|e| AppError::Io { message: e.to_string() })?;
        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    {
        open::that(&path).map_err(|e| AppError::Io { message: e.to_string() })
    }
}

/// Opens the folder containing the file, selecting it if the OS supports it.
#[tauri::command]
pub fn open_folder(path: String) -> Result<(), AppError> {
    #[cfg(target_os = "windows")]
    {
        // /select highlights the file inside Explorer
        std::process::Command::new("explorer")
            .args(["/select,", &path])
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| AppError::Io { message: e.to_string() })?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-R", &path])
            .spawn()
            .map_err(|e| AppError::Io { message: e.to_string() })?;
        return Ok(());
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let folder = std::path::Path::new(&path)
            .parent()
            .ok_or_else(|| AppError::Io { message: format!("Cannot resolve parent folder for: {}", path) })?;
        open::that(folder).map_err(|e| AppError::Io { message: e.to_string() })
    }
}

/// Returns true if the given path is on a network drive.
#[tauri::command]
pub fn is_network_path(path: String) -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        if path.starts_with("\\\\") || path.starts_with("//") {
            return true;
        }

        if path.len() >= 2 && path.as_bytes()[1] == b':' {
            let root = format!("{}:\\", &path[..1].to_uppercase());
            let wide: Vec<u16> = OsStr::new(&root)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let drive_type = unsafe {
                windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(wide.as_ptr())
            };
            return drive_type == 4;
        }
        false
    }
    #[cfg(not(target_os = "windows"))]
    { false }
}

/// Returns the size of the hash cache in bytes, or 0 if not found.
#[tauri::command]
pub fn get_cache_size(app: tauri::AppHandle) -> u64 {
    cache_data_dir(&app).ok()
        .map(|d| d.join("rustymirror_cache.db"))
        .and_then(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .unwrap_or(0)
}

/// Deletes the hash cache database file.
#[tauri::command]
pub fn clear_cache(app: tauri::AppHandle) -> Result<(), AppError> {
    let path = cache_data_dir(&app)?.join("rustymirror_cache.db");
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| AppError::Io { message: e.to_string() })?;
        tracing::debug!(path = %path.display(), "hash cache cleared");
    }
    Ok(())
}

/// Returns the total size of the thumbnail cache directory in bytes.
#[tauri::command]
pub fn get_thumb_cache_size(app: tauri::AppHandle) -> u64 {
    let dir = match cache_data_dir(&app).ok().map(|d| d.join("thumb_cache")) {
        Some(d) => d,
        None    => return 0,
    };
    std::fs::read_dir(&dir).ok()
        .map(|entries| entries
            .filter_map(|e| e.ok())
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len())
            .sum())
        .unwrap_or(0)
}

/// Deletes all cached thumbnails.
#[tauri::command]
pub fn clear_thumb_cache(app: tauri::AppHandle) -> Result<(), AppError> {
    let dir = cache_data_dir(&app)?.join("thumb_cache");
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|e| AppError::Io { message: e.to_string() })?;
        tracing::debug!(path = %dir.display(), "thumb cache cleared");
    }
    Ok(())
}

/// Computes a fingerprint of the given directories based on file paths,
/// sizes and modification times. If nothing has changed since the last scan,
/// this hash will be identical — allowing the frontend to serve cached results.
#[tauri::command]
pub fn directory_fingerprint(
    paths: Vec<String>,
    file_list_cache: State<'_, FileListCache>,
) -> Result<String, AppError> {
    use std::collections::BTreeMap;

    let directories: Vec<std::path::PathBuf> = paths.iter().map(std::path::PathBuf::from).collect();
    let image_paths = crate::scanner::collect_images(&directories);

    // BTreeMap keeps keys sorted — deterministic regardless of OS walk order
    let mut map: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for p in &image_paths {
        if let Ok(meta) = std::fs::metadata(p) {
            let modified = meta.modified().ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            map.insert(p.to_string_lossy().to_string(), (meta.len(), modified));
        }
    }

    // Hash the sorted entries
    let mut hasher = blake3::Hasher::new();
    for (path, (size, mtime)) in &map {
        hasher.update(path.as_bytes());
        hasher.update(&size.to_le_bytes());
        hasher.update(&mtime.to_le_bytes());
    }

    let fingerprint = hasher.finalize().to_hex().to_string();

    let mut by_mtime: Vec<(&String, u64)> = map.iter().map(|(p, &(_, m))| (p, m)).collect();
    by_mtime.sort_by(|a, b| b.1.cmp(&a.1));
    tracing::debug!("fingerprint: {} ({} images)", &fingerprint[..12], map.len());
    for (path, mtime) in by_mtime.iter().take(3) {
        tracing::debug!("  newest: {} (mtime={})", path, mtime);
    }

    // Store the enumerated file list so scan_directories can reuse it.
    *file_list_cache.0.lock().unwrap() = Some((paths, image_paths));

    Ok(fingerprint)
}

#[tauri::command]
pub fn is_debug_build() -> bool {
    cfg!(debug_assertions)
}

/// Scans directories and returns all images with basic file metadata.
/// Used by the metadata editor mode.
#[tauri::command]
pub async fn scan_for_metadata(
    paths: Vec<String>,
    meta_scan_state: State<'_, MetaScanState>,
) -> Result<MetaScanResult, AppError> {
    let stop = Arc::new(AtomicBool::new(false));
    *meta_scan_state.0.lock().unwrap() = stop.clone();

    tokio::task::spawn_blocking(move || {
        use rayon::prelude::*;

        let directories: Vec<std::path::PathBuf> = paths.iter().map(std::path::PathBuf::from).collect();
        let images = crate::scanner::collect_images(&directories);

        let stop_c = stop.clone();
        let results: Vec<Result<crate::types::ImageEntry, FailedFile>> = images
            .into_par_iter()
            .map(|p| {
                if stop_c.load(Ordering::Relaxed) {
                    return Err(FailedFile { path: String::new(), kind: FailedFileKind::IoError }); // sentinel = stopped
                }
                let path_str = p.to_string_lossy().to_string();
                let meta = std::fs::metadata(&p)
                    .map_err(|e| FailedFile { path: path_str.clone(), kind: FailedFileKind::from_io(&e) })?;
                let size_bytes = meta.len();
                let modified = meta.modified().ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .and_then(|d| chrono::DateTime::<chrono::Utc>::from_timestamp(d.as_secs() as i64, 0))
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());

                let lower_ext = p.extension().and_then(|e| e.to_str())
                    .map(|e| e.to_lowercase()).unwrap_or_default();
                let (width, height) = if matches!(lower_ext.as_str(), "heic" | "heif" | "avif") {
                    crate::heic::heic_dimensions(&p, None)
                } else {
                    image::image_dimensions(&p).unwrap_or((0, 0))
                };

                // Read lightweight EXIF for sort fields (BufReader avoids loading the full file)
                let (date_taken, gps_latitude, gps_longitude, device) =
                    std::fs::File::open(&p)
                        .ok()
                        .and_then(|f| {
                            exif::Reader::new()
                                .read_from_container(&mut std::io::BufReader::new(f))
                                .ok()
                        })
                        .map(|exif| {
                            let str_ascii = |tag: exif::Tag| -> Option<String> {
                                exif.get_field(tag, exif::In::PRIMARY).and_then(|f| {
                                    if let exif::Value::Ascii(ref parts) = f.value {
                                        parts.iter()
                                            .filter_map(|p| std::str::from_utf8(p).ok())
                                            .map(|s| s.trim_matches('\0').trim().to_string())
                                            .find(|s| !s.is_empty())
                                    } else { None }
                                })
                            };
                            let date_taken = str_ascii(exif::Tag::DateTimeOriginal)
                                .map(crate::metadata::exif_date_to_iso);
                            let make  = str_ascii(exif::Tag::Make);
                            let model = str_ascii(exif::Tag::Model);
                            let device = match (make, model) {
                                (Some(mk), Some(md)) => Some(format!("{} {}", mk, md)),
                                (Some(mk), None)     => Some(mk),
                                (None,     Some(md)) => Some(md),
                                _                    => None,
                            };
                            let lat = crate::metadata::parse_gps_coord(
                                &exif, exif::Tag::GPSLatitude, exif::Tag::GPSLatitudeRef);
                            let lon = crate::metadata::parse_gps_coord(
                                &exif, exif::Tag::GPSLongitude, exif::Tag::GPSLongitudeRef);
                            (date_taken, lat, lon, device)
                        })
                        .unwrap_or((None, None, None, None));

                Ok(crate::types::ImageEntry {
                    path: path_str,
                    size_bytes,
                    width,
                    height,
                    modified,
                    is_original: false,
                    date_taken,
                    gps_latitude,
                    gps_longitude,
                    blur_score: None,
                    device,
                })
            })
            .collect();

        let was_stopped = stop.load(Ordering::Relaxed);
        let mut entries = Vec::with_capacity(results.len());
        let mut failed_files: Vec<FailedFile> = Vec::new();
        for r in results {
            match r {
                Ok(entry) => entries.push(entry),
                Err(f) if !f.path.is_empty() && !was_stopped => {
                    failed_files.push(f);
                }
                _ => {}
            }
        }

        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(MetaScanResult { images: entries, failed_files })
    })
    .await?
}

/// Reads all EXIF and file metadata for a single image.
#[tauri::command]
pub async fn read_metadata(path: String) -> Result<crate::types::ImageMetadata, AppError> {
    let path_clone = path.clone();
    tokio::task::spawn_blocking(move || {
        crate::metadata::read_metadata(std::path::Path::new(&path_clone))
            .map_err(|e| AppError::Metadata { path: path_clone.clone(), message: e.to_string() })
    })
    .await?
}

/// Writes editable EXIF fields back to an image file and invalidates the
/// SQLite cache entry so the next scan re-reads the updated metadata.
#[tauri::command]
pub async fn write_metadata(
    path: String,
    update: crate::types::MetadataUpdate,
    app: tauri::AppHandle,
) -> Result<(), AppError> {
    let path_clone = path.clone();
    tokio::task::spawn_blocking(move || {
        crate::metadata::write_metadata(std::path::Path::new(&path_clone), &update)
            .map_err(|e| AppError::Metadata { path: path_clone.clone(), message: e.to_string() })
    })
    .await??;

    // Invalidate the SQLite cache entry so the next scan picks up the new date
    if let Ok(data_dir) = cache_data_dir(&app) {
        if let Ok(cache) = crate::cache::Cache::open(&data_dir) {
            cache.evict_deleted(&[path]);
        }
    }

    Ok(())
}
