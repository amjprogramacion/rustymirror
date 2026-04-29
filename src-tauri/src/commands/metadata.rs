use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use rayon::prelude::*;
use tauri::{Emitter, Manager, State};

use super::{AppError, MetaScanResult, MetaScanState, evict_cache_for, to_pathbuf_vec, extract_tag_string, extract_tag_f64, extract_tag_u64};
use crate::types::{AnalyzeProgress, FailedFile, MetaScanProgress};

/// Scans directories and returns all images with basic file metadata.
/// Used by the metadata editor mode.
///
/// Phase 1 — ExifTool daemon: one Perl process is started for the whole scan;
/// chunks of 500 files are sent via stdin, eliminating per-chunk startup overhead.
/// Phase 2 — HEIC parallel: HEIC/HEIF/AVIF entries are corrected in parallel
/// via rayon, since heic_capture_info calls magick externally for each file.
#[tauri::command]
pub async fn scan_for_metadata(
    paths: Vec<String>,
    app: tauri::AppHandle,
    meta_scan_state: State<'_, MetaScanState>,
) -> Result<MetaScanResult, AppError> {
    let stop = Arc::new(AtomicBool::new(false));
    *meta_scan_state.0.lock().unwrap() = stop.clone();
    let resource_dir = app.path().resource_dir().ok();
    let app_handle = app.clone();

    tokio::task::spawn_blocking(move || {
        let directories = to_pathbuf_vec(&paths);
        let all_images = crate::scanner::collect_images(&directories);
        let total = all_images.len();
        let _ = app_handle.emit("meta_scan_progress", MetaScanProgress { total, processed: 0 });

        // GPS and dimension tags use # for raw numeric output.
        const SCAN_TAGS: &[&str] = &[
            "-EXIF:DateTimeOriginal",
            "-Make",
            "-Model",
            "-GPSLatitude#",
            "-GPSLongitude#",
            "-ImageWidth#",
            "-ImageHeight#",
            "-ExifImageWidth#",
            "-ExifImageHeight#",
        ];

        // Start a single long-lived ExifTool daemon for the whole scan.
        // Falls back to empty meta_maps if ExifTool is unavailable.
        let exiftool_path = resource_dir.as_deref().and_then(crate::exiftool::find_exiftool);
        let mut daemon = exiftool_path.as_deref().and_then(|et| {
            crate::exiftool::ExifToolDaemon::start(et)
                .map_err(|e| tracing::warn!("exiftool daemon start failed: {e}"))
                .ok()
        });

        const CHUNK: usize = 500;
        let mut entries: Vec<crate::types::ImageEntry> = Vec::with_capacity(total);
        let mut failed_files: Vec<FailedFile> = Vec::new();
        let mut heic_indices: Vec<usize> = Vec::new();
        let mut processed: usize = 0;

        // ── Phase 1: exiftool daemon queries + per-file entry building ────────
        'outer: for chunk in all_images.chunks(CHUNK) {
            if stop.load(Ordering::Relaxed) { break; }

            // ExifTool returns SourceFile with forward slashes on Windows; normalise
            // both sides to forward slashes for the lookup.
            let meta_map: std::collections::HashMap<String, serde_json::Value> =
                match daemon.as_mut().map(|d| d.batch_query(chunk, SCAN_TAGS)) {
                    Some(Ok(results)) => results
                        .into_iter()
                        .filter_map(|obj| {
                            let src = obj
                                .get("SourceFile")
                                .and_then(|v| v.as_str())
                                .map(|s| s.replace('\\', "/"))?;
                            Some((src, obj))
                        })
                        .collect(),
                    Some(Err(e)) => {
                        tracing::warn!("exiftool daemon query failed: {e}");
                        std::collections::HashMap::new()
                    }
                    None => std::collections::HashMap::new(),
                };

            for p in chunk {
                if stop.load(Ordering::Relaxed) { break 'outer; }

                let path_str = p.to_string_lossy().to_string();
                let fs_meta = match std::fs::metadata(p) {
                    Ok(m) => m,
                    Err(e) => {
                        failed_files.push(FailedFile {
                            path: path_str,
                            kind: crate::types::FailedFileKind::from_io(&e),
                        });
                        processed += 1;
                        let _ = app_handle.emit("meta_scan_progress", MetaScanProgress { total, processed });
                        continue;
                    }
                };

                let size_bytes = fs_meta.len();
                let modified = fs_meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .and_then(|d| {
                        chrono::DateTime::<chrono::Utc>::from_timestamp(d.as_secs() as i64, 0)
                    })
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());

                let lookup_key = path_str.replace('\\', "/");
                let obj = meta_map.get(&lookup_key);

                let width = extract_tag_u64(obj, "ImageWidth")
                    .or_else(|| extract_tag_u64(obj, "ExifImageWidth"))
                    .unwrap_or(0) as u32;
                let height = extract_tag_u64(obj, "ImageHeight")
                    .or_else(|| extract_tag_u64(obj, "ExifImageHeight"))
                    .unwrap_or(0) as u32;

                let is_heic = p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| matches!(e.to_lowercase().as_str(), "heic" | "heif" | "avif"))
                    .unwrap_or(false);

                // For HEIC/HEIF/AVIF, defer dimension and date corrections to Phase 2
                // (parallel heic_capture_info) which reads local-time DateTimeOriginal
                // from the EXIF block instead of QuickTime UTC.
                if is_heic {
                    heic_indices.push(entries.len());
                }

                let date_taken = extract_tag_string(obj, "DateTimeOriginal")
                    .map(crate::metadata::exif_date_to_iso);

                let make = extract_tag_string(obj, "Make");
                let model = extract_tag_string(obj, "Model");
                let device = match (make, model) {
                    (Some(mk), Some(md)) => Some(format!("{mk} {md}")),
                    (Some(mk), None) => Some(mk),
                    (None, Some(md)) => Some(md),
                    _ => None,
                };

                entries.push(crate::types::ImageEntry {
                    path: path_str,
                    size_bytes,
                    width,
                    height,
                    modified,
                    is_original: false,
                    date_taken,
                    gps_latitude: extract_tag_f64(obj, "GPSLatitude"),
                    gps_longitude: extract_tag_f64(obj, "GPSLongitude"),
                    blur_score: None,
                    device,
                });
                processed += 1;
                let _ = app_handle.emit("meta_scan_progress", MetaScanProgress { total, processed });
            }
        }

        drop(daemon); // sends -stay_open False, waits for exiftool to exit

        if stop.load(Ordering::Relaxed) {
            return Err(AppError::Scan { message: "scan stopped".into() });
        }

        // ── Phase 2: HEIC/HEIF/AVIF correction in parallel ───────────────────
        // heic_capture_info calls magick externally per file — parallelising with
        // rayon gives near-linear speedup on multi-core machines.
        if !heic_indices.is_empty() {
            let heic_total = heic_indices.len();
            let heic_done = AtomicUsize::new(0);

            // Emit the initial HEIC phase event before starting rayon so the
            // frontend transitions straight from the scan bar to the analyze bar
            // without going through the indeterminate fallback.
            let _ = app_handle.emit("meta_analyze_progress", AnalyzeProgress {
                analyzed: 0,
                total: heic_total,
                phase: "Correcting HEICs…".into(),
            });

            let corrections: Vec<(usize, u32, u32, Option<String>)> = heic_indices
                .par_iter()
                .filter_map(|&idx| {
                    if stop.load(Ordering::Relaxed) { return None; }
                    let path = std::path::Path::new(&entries[idx].path);
                    let (w, h, date) = crate::heic::heic_capture_info(path, resource_dir.as_deref());
                    let done = heic_done.fetch_add(1, Ordering::Relaxed) + 1;
                    let _ = app_handle.emit("meta_analyze_progress", AnalyzeProgress {
                        analyzed: done,
                        total: heic_total,
                        phase: "Correcting HEICs…".into(),
                    });
                    if w == 0 && date.is_none() { return None; }
                    Some((idx, w, h, date))
                })
                .collect();

            for (idx, w, h, date) in corrections {
                let e = &mut entries[idx];
                if w > 0 { e.width = w; e.height = h; }
                e.date_taken = date;
            }
        }

        if stop.load(Ordering::Relaxed) {
            return Err(AppError::Scan { message: "scan stopped".into() });
        }

        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(MetaScanResult { images: entries, failed_files })
    })
    .await?
}

/// Reads all EXIF and file metadata for a single image.
#[tauri::command]
pub async fn read_metadata(
    path: String,
    app: tauri::AppHandle,
) -> Result<crate::types::ImageMetadata, AppError> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|e| AppError::Metadata { path: path.clone(), message: e.to_string() })?;
    let path_clone = path.clone();
    tokio::task::spawn_blocking(move || {
        crate::metadata::read_metadata(
            std::path::Path::new(&path_clone),
            &resource_dir,
        )
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
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|e| AppError::Metadata { path: path.clone(), message: e.to_string() })?;
    let path_clone = path.clone();
    tokio::task::spawn_blocking(move || {
        crate::metadata::write_metadata(
            std::path::Path::new(&path_clone),
            &update,
            &resource_dir,
        )
        .map_err(|e| AppError::Metadata { path: path_clone.clone(), message: e.to_string() })
    })
    .await??;

    // Invalidate the SQLite cache entry so the next scan picks up the new date
    evict_cache_for(&app, &[path]);

    Ok(())
}
