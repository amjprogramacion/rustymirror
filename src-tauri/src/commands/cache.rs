use super::{AppError, cache_data_dir};

/// Returns the size of the hash cache in bytes, or 0 if not found.
/// Includes the WAL and SHM sidecar files so the reported size reflects
/// all data written since the last checkpoint, not just the main db file.
#[tauri::command]
pub fn get_cache_size(app: tauri::AppHandle) -> u64 {
    let base = match cache_data_dir(&app).ok().map(|d| d.join("rustymirror_cache.db")) {
        Some(p) => p,
        None    => return 0,
    };
    let file_size = |suffix: &str| -> u64 {
        let mut p = base.clone();
        let name = format!("{}{}", base.file_name().unwrap_or_default().to_string_lossy(), suffix);
        p.set_file_name(name);
        std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
    };
    file_size("") + file_size("-wal") + file_size("-shm")
}

/// Deletes the hash cache database file and its WAL/SHM sidecars.
#[tauri::command]
pub fn clear_cache(app: tauri::AppHandle) -> Result<(), AppError> {
    let base = cache_data_dir(&app)?.join("rustymirror_cache.db");
    for suffix in &["", "-wal", "-shm"] {
        let mut p = base.clone();
        let name = format!("{}{}", base.file_name().unwrap_or_default().to_string_lossy(), suffix);
        p.set_file_name(name);
        if p.exists() {
            std::fs::remove_file(&p)
                .map_err(|e| AppError::Io { message: e.to_string() })?;
        }
    }
    tracing::debug!(path = %base.display(), "hash cache cleared");
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

/// Returns whether this is a debug build.
#[tauri::command]
pub fn is_debug_build() -> bool {
    cfg!(debug_assertions)
}

/// Flushes the cache WAL (Write-Ahead Log) to disk.
/// Ensures all pending cache writes are persisted to the database file.
/// Useful before app shutdown to guarantee data durability.
#[tauri::command]
pub fn flush_cache(app: tauri::AppHandle) -> Result<(), AppError> {
    cache_data_dir(&app).ok()
        .and_then(|d| crate::cache::Cache::open(&d).ok())
        .ok_or_else(|| AppError::Io { message: "Cache not available".to_string() })?
        .flush()
        .map_err(|e| AppError::Io { message: e.to_string() })
}
