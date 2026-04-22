mod scan;
mod thumbnail;
mod metadata;
mod organizer;
mod media;
mod file;
mod cache;

// Re-export all commands
pub use scan::*;
pub use thumbnail::*;
pub use metadata::*;
pub use organizer::*;
pub use media::*;
pub use file::*;
pub use cache::*;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;
use tauri::{AppHandle, Manager};

/// Returns the cache directory for the current build mode.
/// In debug builds, uses a `dev` subdirectory to keep dev caches separate from release caches.
pub fn cache_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, tauri::Error> {
    let base = app.path().app_data_dir()?;
    if cfg!(debug_assertions) {
        Ok(base.join("dev"))
    } else {
        Ok(base)
    }
}

/// Evicts the given paths from the SQLite cache. Silently ignores errors so callers
/// don't need to handle the "cache unavailable" case separately.
pub fn evict_cache_for(app: &tauri::AppHandle, paths: &[String]) {
    if let Ok(data_dir) = cache_data_dir(app) {
        if let Ok(cache) = crate::cache::Cache::open(&data_dir) {
            cache.evict_deleted(paths);
        }
    }
}

/// Runs an organizer job on a blocking thread with the standard boilerplate:
/// stop flag installation, resource_dir lookup, path conversion, and AppHandle cloning.
/// The closure receives everything it needs to invoke the actual organizer function
/// and emit progress events.
pub async fn run_organizer_job<T, F>(
    paths: Vec<String>,
    app: AppHandle,
    state: tauri::State<'_, OrganizerState>,
    job: F,
) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce(Vec<PathBuf>, Option<PathBuf>, Arc<AtomicBool>, AppHandle) -> T + Send + 'static,
{
    let stop = Arc::new(AtomicBool::new(false));
    *state.0.lock().unwrap() = stop.clone();
    let resource_dir = app.path().resource_dir().ok();
    let directories: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let app_c = app.clone();
    tokio::task::spawn_blocking(move || job(directories, resource_dir, stop, app_c))
        .await
        .map_err(Into::into)
}

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
    /// Organizer operation failure.
    Organizer { message: String },
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
    pub groups: Vec<crate::types::DuplicateGroup>,
    pub failed_files: Vec<crate::types::FailedFile>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaScanResult {
    pub images: Vec<crate::types::ImageEntry>,
    pub failed_files: Vec<crate::types::FailedFile>,
}

/// Shared atomic stop flag — set to true when the user clicks "Stop scan".
/// Wrapped in Mutex<Arc<...>> so we can replace it for each new scan.
pub struct ScanState(pub Mutex<Arc<AtomicBool>>);

/// Same pattern for the metadata scan.
pub struct MetaScanState(pub Mutex<Arc<AtomicBool>>);

/// Caches the file list from directory_fingerprint so scan_directories can
/// reuse it without a second SMB traversal (avoids count discrepancy on NAS).
pub struct FileListCache(pub Mutex<Option<(Vec<String>, Vec<PathBuf>)>>);

/// Shared atomic stop flag for organizer operations.
pub struct OrganizerState(pub Mutex<Arc<AtomicBool>>);
