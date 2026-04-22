use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Manager, State};

use super::{AppError, MetaScanResult, MetaScanState, evict_cache_for, to_pathbuf_vec, extract_tag_string, extract_tag_f64, extract_tag_u64};
use crate::types::FailedFile;

/// Scans directories and returns all images with basic file metadata.
/// Used by the metadata editor mode.
///
/// Uses ExifTool in batch mode (one process call per chunk of 500 files) for
/// fast metadata extraction across all supported formats including HEIC/AVIF.
#[tauri::command]
pub async fn scan_for_metadata(
    paths: Vec<String>,
    app: tauri::AppHandle,
    meta_scan_state: State<'_, MetaScanState>,
) -> Result<MetaScanResult, AppError> {
    let stop = Arc::new(AtomicBool::new(false));
    *meta_scan_state.0.lock().unwrap() = stop.clone();
    let resource_dir = app.path().resource_dir().ok();

    tokio::task::spawn_blocking(move || {
        let directories = to_pathbuf_vec(&paths);
        let all_images = crate::scanner::collect_images(&directories);

        let exiftool = resource_dir
            .as_deref()
            .and_then(crate::exiftool::find_exiftool);

        // Lightweight tag set for the metadata scan (sort fields only).
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

        const CHUNK: usize = 500;
        let mut entries: Vec<crate::types::ImageEntry> = Vec::with_capacity(all_images.len());
        let mut failed_files: Vec<FailedFile> = Vec::new();

        for chunk in all_images.chunks(CHUNK) {
            if stop.load(Ordering::Relaxed) {
                break;
            }

            // Build a normalised-path → JSON-object map from the exiftool batch call.
            // ExifTool returns SourceFile with forward slashes on Windows, so we
            // normalise both sides to forward slashes for the lookup.
            let meta_map: std::collections::HashMap<String, serde_json::Value> =
                if let Some(ref et) = exiftool {
                    match crate::exiftool::batch_read_tags(et, chunk, SCAN_TAGS) {
                        Ok(results) => results
                            .into_iter()
                            .filter_map(|obj| {
                                let src = obj
                                    .get("SourceFile")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.replace('\\', "/"))?;
                                Some((src, obj))
                            })
                            .collect(),
                        Err(e) => {
                            tracing::warn!("exiftool batch scan failed: {e}");
                            std::collections::HashMap::new()
                        }
                    }
                } else {
                    std::collections::HashMap::new()
                };

            for p in chunk {
                let path_str = p.to_string_lossy().to_string();
                let fs_meta = match std::fs::metadata(p) {
                    Ok(m) => m,
                    Err(e) => {
                        failed_files.push(FailedFile {
                            path: path_str,
                            kind: crate::types::FailedFileKind::from_io(&e),
                        });
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

                let mut width = extract_tag_u64(obj, "ImageWidth")
                    .or_else(|| extract_tag_u64(obj, "ExifImageWidth"))
                    .unwrap_or(0) as u32;
                let mut height = extract_tag_u64(obj, "ImageHeight")
                    .or_else(|| extract_tag_u64(obj, "ExifImageHeight"))
                    .unwrap_or(0) as u32;

                let is_heic_format = p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| matches!(e.to_lowercase().as_str(), "heic" | "heif" | "avif"))
                    .unwrap_or(false);

                // For HEIC/HEIF/AVIF use the shared helper (heic_capture_info):
                // converts to a small temp JPEG via magick and reads dimensions
                // and DateTimeOriginal from the embedded EXIF block (local
                // wall-clock time, not QuickTime UTC).
                let date_taken: Option<String> = if is_heic_format {
                    let (w, h, date) = crate::heic::heic_capture_info(p, resource_dir.as_deref());
                    if w > 0 { width = w; height = h; }
                    date
                } else {
                    extract_tag_string(obj, "DateTimeOriginal").map(crate::metadata::exif_date_to_iso)
                };

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
            }
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
