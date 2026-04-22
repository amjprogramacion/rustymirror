use tauri::Emitter;

use super::{AppError, evict_cache_for};

#[tauri::command]
pub async fn delete_files(paths: Vec<String>, app: tauri::AppHandle) -> Result<(), AppError> {
    let total = paths.len();
    tracing::debug!("delete_files: {} files", total);

    #[cfg(target_os = "windows")]
    {
        for (i, path) in paths.iter().enumerate() {
            let p = path.trim();
            if p.is_empty() { continue; }

            // Network paths (UNC or mapped drive) have no recycle bin — delete permanently.
            // Local paths go to the recycle bin via the trash crate.
            let is_network = is_network_path(p.to_string());
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

        evict_cache_for(&app, &paths);
        tracing::debug!(count = paths.len(), "cache entries evicted");

        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    {
        for (i, path) in paths.iter().enumerate() {
            trash::delete(path)
                .map_err(|e| AppError::Delete { path: path.clone(), message: e.to_string() })?;
            let _ = app.emit("delete_progress", serde_json::json!({ "done": i + 1, "total": total }));
        }
        evict_cache_for(&app, &paths);
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

/// Opens a file with its default application.
/// On Windows, delegating to explorer.exe avoids the security zone warning
/// dialog that appears when launching files via ShellExecuteW from an
/// unsigned process — explorer.exe is a trusted system process.
#[tauri::command]
pub fn open_file(path: String) -> Result<(), AppError> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

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
        use std::os::windows::process::CommandExt;

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

/// Checks whether each path in `paths` exists on disk.
/// Returns one bool per path. Runs in Rust so it bypasses frontend fs scope restrictions.
#[tauri::command]
pub fn check_paths_exist(paths: Vec<String>) -> Vec<bool> {
    paths.iter().map(|p| std::path::Path::new(p).exists()).collect()
}
