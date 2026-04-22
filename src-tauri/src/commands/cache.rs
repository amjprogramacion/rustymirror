use super::{AppError, cache_data_dir};

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

/// Returns whether this is a debug build.
#[tauri::command]
pub fn is_debug_build() -> bool {
    cfg!(debug_assertions)
}
