use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::scanner::{apply_retention_rule, find_duplicates};
use crate::types::{AnalyzeProgress, RetentionRule, ScanProgress};
use super::{AppError, ScanResult, ScanState, MetaScanState, FileListCache, cache_data_dir};

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

    let directories: Vec<std::path::PathBuf> = paths.iter().map(std::path::PathBuf::from).collect();
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
    groups: Vec<crate::types::DuplicateGroup>,
    rule: RetentionRule,
) -> Vec<crate::types::DuplicateGroup> {
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
