use std::path::PathBuf;
use std::sync::{Arc, Mutex};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter, Manager, State};

use crate::scanner::find_duplicates;
use crate::types::{AnalyzeProgress, DuplicateGroup, ScanProgress};

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
    app: AppHandle,
    scan_state: State<'_, ScanState>,
    file_list_cache: State<'_, FileListCache>,
) -> Result<Vec<DuplicateGroup>, String> {
    let stop = Arc::new(AtomicBool::new(false));
    *scan_state.0.lock().unwrap() = stop.clone();

    let directories: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let app_clone  = app.clone();
    let app_clone2 = app.clone();
    let resource_dir = app.path().resource_dir().ok();
    let cache = app.path().app_data_dir().ok()
        .and_then(|d| crate::cache::Cache::open(&d).ok())
        .map(std::sync::Arc::new);
    log::debug!("[RustyMirror:RS] cache: {}", if cache.is_some() { "ok" } else { "unavailable" });
    log::debug!("[RustyMirror:RS] scan_directories called with {} paths", paths.len());
    for p in &paths { log::debug!("[RustyMirror:RS]   path: {}", p); }

    let threshold  = hamming_threshold.unwrap_or(6);
    let use_fast   = fast_mode.unwrap_or(false);
    log::debug!("[RustyMirror:RS] hamming threshold: {}", threshold);
    log::debug!("[RustyMirror:RS] fast mode: {}", use_fast);

    // Reuse the file list from directory_fingerprint if it matches the current paths,
    // to avoid a second SMB directory traversal that may return fewer files (NAS cache cold).
    let prefetched = {
        let mut guard = file_list_cache.0.lock().unwrap();
        if let Some((cached_paths, file_list)) = guard.take() {
            if cached_paths == paths {
                log::debug!("[RustyMirror:RS] reusing {} prefetched paths from fingerprint", file_list.len());
                Some(file_list)
            } else {
                None
            }
        } else {
            None
        }
    };

    let result = tokio::task::spawn_blocking(move || {
        log::debug!("[RustyMirror:RS] spawn_blocking started");
        let r = find_duplicates(
            directories,
            prefetched,
            resource_dir,
            stop,
            threshold,
            cache,
            cross_date_phash.unwrap_or(true),
            use_fast,
            move |scanned, total| {
                if scanned == 1 || scanned % 50 == 0 || scanned == total {
                    log::debug!("[RustyMirror:RS] progress {}/{}", scanned, total);
                }
                let _ = app_clone.emit("scan_progress", ScanProgress { scanned, total });
            },
            move |progress: AnalyzeProgress| {
                let _ = app_clone2.emit("analyze_progress", &progress);
            },
        );
        log::debug!("[RustyMirror:RS] find_duplicates returned: {}", if r.is_ok() { "Ok" } else { "Err" });
        r
    })
    .await;

    log::debug!("[RustyMirror:RS] spawn_blocking joined: {}", match &result { Ok(_) => "Ok", Err(_) => "JoinError" });

    result
        .map_err(|e| { let s = e.to_string(); log::debug!("[RustyMirror:RS] JoinError: {}", s); s })?
        .map_err(|e| { let s = e.to_string(); log::debug!("[RustyMirror:RS] ScanError: {}", s); s })
}

#[tauri::command]
pub fn stop_scan(scan_state: State<'_, ScanState>) {
    log::debug!("[RustyMirror:RS] stop requested");
    scan_state.0.lock().unwrap().store(true, Ordering::Relaxed);
}

#[tauri::command]
pub fn stop_meta_scan(meta_scan_state: State<'_, MetaScanState>) {
    log::debug!("[RustyMirror:RS] meta scan stop requested");
    meta_scan_state.0.lock().unwrap().store(true, Ordering::Relaxed);
}

#[tauri::command]
pub async fn delete_files(paths: Vec<String>, app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Emitter;
    let total = paths.len();
    log::debug!("[RustyMirror:RS] delete_files: {} files", total);

    #[cfg(target_os = "windows")]
    {
        // Write paths to a temp file (UTF-8) to avoid any shell quoting issues
        let tmp_list = std::env::temp_dir().join("rustymirror_delete_list.txt");
        std::fs::write(&tmp_list, paths.join("\n"))
            .map_err(|e| format!("Failed to write path list: {}", e))?;

        // Escape single quotes in the temp path for PowerShell
        let tmp_str = tmp_list.to_string_lossy().replace('\'', "''");

        // Key fixes:
        //   1. -ExecutionPolicy Bypass — avoids silent block by corporate policies
        //   2. Network paths (UNC \\...) use Remove-Item (permanent delete)
        //   3. Local paths use SendToRecycleBin via VisualBasic FileSystem
        //   4. Always log stdout/stderr so we can detect done=0 silently
        let script = format!(
            r#"$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName Microsoft.VisualBasic
$lines = Get-Content -LiteralPath '{tmp}' -Encoding UTF8
$i = 0
foreach ($p in $lines) {{
    $p = $p.Trim()
    if ($p -eq '') {{ continue }}
    if ($p.StartsWith('\\') -or $p.StartsWith('//')) {{
        Remove-Item -LiteralPath $p -Force
    }} else {{
        [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile(
            $p,
            [Microsoft.VisualBasic.FileIO.UIOption]::OnlyErrorDialogs,
            [Microsoft.VisualBasic.FileIO.RecycleOption]::SendToRecycleBin
        )
    }}
    $i++
    Write-Output "PROGRESS:$i"
}}"#,
            tmp = tmp_str
        );

        let output = tokio::task::spawn_blocking(move || {
            std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy", "Bypass",
                    "-Command", &script,
                ])
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .output()
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("PowerShell launch failed: {}", e))?;

        let _ = std::fs::remove_file(&tmp_list);

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        log::debug!("[RustyMirror:RS] ps stdout: {}", stdout.trim());
        if !stderr.trim().is_empty() {
            log::debug!("[RustyMirror:RS] ps stderr: {}", stderr.trim());
        }

        if !output.status.success() {
            let msg = format!("Delete failed: {}", stderr.trim());
            log::debug!("[RustyMirror:RS] ERROR: {}", msg);
            return Err(msg);
        }

        let mut done = 0usize;
        for line in stdout.lines() {
            if let Some(n) = line.strip_prefix("PROGRESS:") {
                if let Ok(i) = n.trim().parse::<usize>() {
                    done = i;
                    let _ = app.emit("delete_progress", serde_json::json!({ "done": done, "total": total }));
                }
            }
        }
        log::debug!("[RustyMirror:RS] deleted {} / {} files", done, total);

        // If PowerShell exited 0 but deleted nothing, treat it as a real error
        if done == 0 && total > 0 {
            return Err(format!(
                "PowerShell ran but deleted 0 files. stderr: {}",
                stderr.trim()
            ));
        }

        if let Ok(data_dir) = app.path().app_data_dir() {
            if let Ok(cache) = crate::cache::Cache::open(&data_dir) {
                cache.evict_deleted(&paths);
                log::debug!("[RustyMirror:RS] evicted {} entries from cache", paths.len());
            }
        }

        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    {
        for (i, path) in paths.iter().enumerate() {
            trash::delete(path).map_err(|e| format!("Failed to delete '{}': {}", path, e))?;
            let _ = app.emit("delete_progress", serde_json::json!({ "done": i + 1, "total": total }));
        }
        if let Ok(data_dir) = app.path().app_data_dir() {
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
        _       => log::debug!("[RustyMirror:JS] INFO  — {}", message),
    }
}

#[tauri::command]
pub async fn get_thumbnail(path: String, app: tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;

    let resource_dir    = app.path().resource_dir().ok();
    let thumb_cache_dir = app.path().app_data_dir().ok().map(|d| d.join("thumb_cache"));

    tokio::task::spawn_blocking(move || {
        use image::imageops::FilterType;
        use std::io::{Cursor, Seek, SeekFrom};
        use base64::Engine;

        let lower   = path.to_lowercase();
        let is_heic = lower.ends_with(".heic") || lower.ends_with(".heif");

        if is_heic {
            let cache_path = thumb_cache_dir.as_ref().and_then(|dir| {
                let bytes = std::fs::read(&path).ok()?;
                let hash  = blake3::hash(&bytes);
                let name  = format!("{}.jpg", &hash.to_hex()[..16]);
                Some(dir.join(name))
            });

            if let Some(ref cp) = cache_path {
                if cp.exists() {
                    if let Ok(cached) = std::fs::read(cp) {
                        log::debug!("[RustyMirror:RS] thumb HIT (heic): {}", path);
                        return Ok(format!("data:image/jpeg;base64,{}",
                            base64::engine::general_purpose::STANDARD.encode(&cached)));
                    }
                }
            }

            log::debug!("[RustyMirror:RS] thumb MISS (heic): {}", path);
            let (tmp, _, _) = crate::heic::heic_to_temp_jpeg(
                std::path::Path::new(&path),
                resource_dir.as_deref(),
            ).ok_or_else(|| "heic-no-converter".to_string())?;

            let bytes = std::fs::read(&tmp).map_err(|e| e.to_string())?;
            let _ = std::fs::remove_file(&tmp);

            let img   = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
            let thumb = img.resize(180, 180, FilterType::Nearest);
            let mut buf = Cursor::new(Vec::<u8>::new());
            thumb.write_to(&mut buf, image::ImageFormat::Jpeg).map_err(|e| e.to_string())?;
            buf.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
            let thumb_bytes = buf.into_inner();

            if let Some(ref cp) = cache_path {
                if let Some(parent) = cp.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(cp, &thumb_bytes);
                log::debug!("[RustyMirror:RS] thumb SAVE (heic): {}", path);
            }

            return Ok(format!("data:image/jpeg;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&thumb_bytes)));
        }

        // Non-HEIC: only process network paths (local files use convertFileSrc)
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;

        let cache_path = thumb_cache_dir.as_ref().and_then(|dir| {
            let hash = blake3::hash(&bytes);
            let name = format!("jpg_{}.jpg", &hash.to_hex()[..16]);
            Some(dir.join(name))
        });

        if let Some(ref cp) = cache_path {
            if cp.exists() {
                if let Ok(cached) = std::fs::read(cp) {
                    log::debug!("[RustyMirror:RS] thumb HIT (jpg/net): {}", path);
                    return Ok(format!("data:image/jpeg;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(&cached)));
                }
            }
        }

        log::debug!("[RustyMirror:RS] thumb MISS (jpg/net): {}", path);

        let img = match image::load_from_memory(&bytes) {
            Ok(img) => img,
            Err(e) => {
                // The image crate failed to decode the file (unusual PNG variant,
                // unsupported bit depth, etc.). Return the raw bytes as a data URI
                // so the browser can still try to render it natively.
                log::warn!("[RustyMirror:RS] thumb decode failed ({}): {} — returning raw", e, path);
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
        thumb.write_to(&mut buf, image::ImageFormat::Jpeg).map_err(|e| e.to_string())?;
        buf.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let thumb_bytes = buf.into_inner();

        if let Some(ref cp) = cache_path {
            if let Some(parent) = cp.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(cp, &thumb_bytes);
            log::debug!("[RustyMirror:RS] thumb SAVE (jpg/net): {}", path);
        }

        Ok(format!("data:image/jpeg;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(thumb_bytes)))
    })
    .await
    .map_err(|e| e.to_string())?
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
pub async fn get_full_image(path: String, app: tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    let resource_dir = app.path().resource_dir().ok();

    tokio::task::spawn_blocking(move || {
        use std::io::{Cursor, Seek, SeekFrom};
        use base64::Engine;

        let lower   = path.to_lowercase();
        let is_heic = lower.ends_with(".heic") || lower.ends_with(".heif");

        let bytes = if is_heic {
            let (tmp, _, _) = crate::heic::heic_to_temp_jpeg(
                std::path::Path::new(&path),
                resource_dir.as_deref(),
            ).ok_or_else(|| "heic-no-converter".to_string())?;
            let b = std::fs::read(&tmp).map_err(|e| e.to_string())?;
            let _ = std::fs::remove_file(&tmp);
            b
        } else {
            std::fs::read(&path).map_err(|e| e.to_string())?
        };

        let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
        let img = if !is_heic { apply_exif_orientation(&bytes, img) } else { img };

        let mut buf = Cursor::new(Vec::<u8>::new());
        img.write_to(&mut buf, image::ImageFormat::Jpeg).map_err(|e| e.to_string())?;
        buf.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;

        Ok(format!("data:image/jpeg;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(buf.into_inner())))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Opens a file with its default application.
/// On Windows, delegating to explorer.exe avoids the security zone warning
/// dialog that appears when launching files via ShellExecuteW from an
/// unsigned process — explorer.exe is a trusted system process.
#[tauri::command]
pub fn open_file(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&path)
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    {
        open::that(&path).map_err(|e| e.to_string())
    }
}

/// Opens the folder containing the file, selecting it if the OS supports it.
#[tauri::command]
pub fn open_folder(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // /select highlights the file inside Explorer
        std::process::Command::new("explorer")
            .args(["/select,", &path])
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-R", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let folder = std::path::Path::new(&path)
            .parent()
            .ok_or_else(|| format!("Cannot resolve parent folder for: {}", path))?;
        open::that(folder).map_err(|e| e.to_string())
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
    use tauri::Manager;
    app.path().app_data_dir().ok()
        .map(|d| d.join("rustymirror_cache.db"))
        .and_then(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .unwrap_or(0)
}

/// Deletes the hash cache database file.
#[tauri::command]
pub fn clear_cache(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let path = app.path().app_data_dir()
        .map_err(|e| e.to_string())?
        .join("rustymirror_cache.db");
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        log::debug!("[RustyMirror:RS] cache deleted: {}", path.display());
    }
    Ok(())
}

/// Returns the total size of the thumbnail cache directory in bytes.
#[tauri::command]
pub fn get_thumb_cache_size(app: tauri::AppHandle) -> u64 {
    use tauri::Manager;
    let dir = match app.path().app_data_dir().ok().map(|d| d.join("thumb_cache")) {
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
pub fn clear_thumb_cache(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let dir = app.path().app_data_dir()
        .map_err(|e| e.to_string())?
        .join("thumb_cache");
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
        log::debug!("[RustyMirror:RS] thumb cache cleared: {}", dir.display());
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
) -> Result<String, String> {
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
    log::debug!("[RustyMirror:RS] fingerprint: {} ({} images)", &fingerprint[..12], map.len());
    for (path, mtime) in by_mtime.iter().take(3) {
        log::debug!("[RustyMirror:RS]   newest: {} (mtime={})", path, mtime);
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
) -> Result<Vec<crate::types::ImageEntry>, String> {
    let stop = Arc::new(AtomicBool::new(false));
    *meta_scan_state.0.lock().unwrap() = stop.clone();

    tokio::task::spawn_blocking(move || {
        use rayon::prelude::*;

        let directories: Vec<std::path::PathBuf> = paths.iter().map(std::path::PathBuf::from).collect();
        let images = crate::scanner::collect_images(&directories);

        let mut entries: Vec<crate::types::ImageEntry> = images
            .into_par_iter()
            .filter_map(|p| {
                if stop.load(Ordering::Relaxed) { return None; }
                let meta = std::fs::metadata(&p).ok()?;
                let size_bytes = meta.len();
                let modified = meta.modified().ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .and_then(|d| chrono::DateTime::<chrono::Utc>::from_timestamp(d.as_secs() as i64, 0))
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());

                let (width, height) = image::image_dimensions(&p).unwrap_or((0, 0));

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

                Some(crate::types::ImageEntry {
                    path: p.to_string_lossy().to_string(),
                    size_bytes,
                    width,
                    height,
                    modified,
                    is_original: false,
                    date_taken,
                    gps_latitude,
                    gps_longitude,
                    device,
                })
            })
            .collect();

        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(entries)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Reads all EXIF and file metadata for a single image.
#[tauri::command]
pub async fn read_metadata(path: String) -> Result<crate::types::ImageMetadata, String> {
    tokio::task::spawn_blocking(move || {
        crate::metadata::read_metadata(std::path::Path::new(&path))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Writes editable EXIF fields back to an image file and invalidates the
/// SQLite cache entry so the next scan re-reads the updated metadata.
#[tauri::command]
pub async fn write_metadata(
    path: String,
    update: crate::types::MetadataUpdate,
    app: tauri::AppHandle,
) -> Result<(), String> {
    use tauri::Manager;
    let path_clone = path.clone();
    tokio::task::spawn_blocking(move || {
        crate::metadata::write_metadata(std::path::Path::new(&path_clone), &update)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

    // Invalidate the SQLite cache entry so the next scan picks up the new date
    if let Ok(data_dir) = app.path().app_data_dir() {
        if let Ok(cache) = crate::cache::Cache::open(&data_dir) {
            cache.evict_deleted(&[path]);
        }
    }

    Ok(())
}
