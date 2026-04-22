use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, State};

use super::{AppError, OrganizerState, run_organizer_job};

/// Returns a preview of what the organizer would do to each file (no changes made).
/// Files are processed in the order provided (respects UI sorting), not re-sorted.
#[tauri::command]
pub async fn preview_organize(
    paths: Vec<String>,
    config: crate::organizer::OrganizerConfig,
    app: AppHandle,
    state: State<'_, OrganizerState>,
) -> Result<Vec<crate::organizer::OrganizerFileAction>, AppError> {
    run_organizer_job(paths, app, state, move |dirs, res_dir, stop, app_c| {
        let exiftool = res_dir.as_deref().and_then(crate::exiftool::find_exiftool);
        crate::organizer::preview_files_ordered(
            &dirs,
            &config,
            exiftool.as_deref(),
            stop,
            |p| { let _ = app_c.emit("organize_progress", &p); },
        )
    })
    .await
}

/// Returns a preview of what dates would be written by execute_metadata_rewrite.
/// Files are processed in the order provided (respects UI sorting), not re-sorted.
#[tauri::command]
pub async fn preview_rewrite_date(
    paths: Vec<String>,
    config: crate::organizer::OrganizerConfig,
    app: AppHandle,
    state: State<'_, OrganizerState>,
) -> Result<Vec<crate::organizer::RewriteDateAction>, AppError> {
    run_organizer_job(paths, app, state, move |dirs, res_dir, stop, app_c| {
        let exiftool = res_dir.as_deref().and_then(crate::exiftool::find_exiftool);
        crate::organizer::preview_rewrite_metadata_ordered(
            &dirs,
            &config,
            exiftool.as_deref(),
            stop,
            |p| { let _ = app_c.emit("organize_progress", &p); },
        )
    })
    .await
}

/// Renames (and optionally moves) files according to the organizer config.
#[tauri::command]
pub async fn execute_organize(
    paths: Vec<String>,
    config: crate::organizer::OrganizerConfig,
    app: AppHandle,
    state: State<'_, OrganizerState>,
) -> Result<crate::organizer::OrganizerSummary, AppError> {
    if !config.only_rename && config.output_directory.trim().is_empty() {
        return Err(AppError::Organizer {
            message: "Output directory is required when 'Only rename' is disabled".to_string(),
        });
    }
    run_organizer_job(paths, app, state, move |dirs, res_dir, stop, app_c| {
        let exiftool = res_dir.as_deref().and_then(crate::exiftool::find_exiftool);
        crate::organizer::execute(
            &dirs,
            &config,
            exiftool.as_deref(),
            stop,
            |p| { let _ = app_c.emit("organize_progress", &p); },
        )
    })
    .await
}

/// Rewrites EXIF date tags on all files using the best available date.
#[tauri::command]
pub async fn execute_metadata_rewrite(
    paths: Vec<String>,
    config: crate::organizer::OrganizerConfig,
    app: AppHandle,
    state: State<'_, OrganizerState>,
) -> Result<crate::organizer::OrganizerSummary, AppError> {
    run_organizer_job(paths, app, state, move |dirs, res_dir, stop, app_c| {
        let exiftool = res_dir.as_deref().and_then(crate::exiftool::find_exiftool);
        crate::organizer::rewrite_metadata(
            &dirs,
            &config,
            exiftool.as_deref(),
            stop,
            |p| { let _ = app_c.emit("organize_progress", &p); },
        )
    })
    .await
}

/// Stops any in-progress organizer operation.
#[tauri::command]
pub fn stop_organize(state: State<'_, OrganizerState>) {
    state.0.lock().unwrap().store(true, Ordering::Relaxed);
}
