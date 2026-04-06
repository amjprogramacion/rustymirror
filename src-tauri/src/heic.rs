//! HEIC support: metadata extraction and JPEG conversion via magick.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use anyhow::{Context, Result};

// ── Cached magick path ────────────────────────────────────────────────────────
static MAGICK_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

pub fn magick_path(resource_dir: Option<&Path>) -> Option<&'static PathBuf> {
    MAGICK_PATH.get_or_init(|| find_magick(resource_dir)).as_ref()
}

fn find_magick(resource_dir: Option<&Path>) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(res) = resource_dir {
            candidates.push(res.join("magick.exe"));
            if let Some(src_tauri) = res.parent().and_then(|p| p.parent()) {
                candidates.push(src_tauri.join("resources").join("magick.exe"));
            }
        }
        candidates.push(PathBuf::from("src-tauri").join("resources").join("magick.exe"));

        for candidate in &candidates {
            log::debug!("[RustyMirror:RS] checking magick at: {}", candidate.display());
            if candidate.exists() {
                log::debug!("[RustyMirror:RS] magick found: {}", candidate.display());
                return Some(candidate.clone());
            }
        }
        if which_exists("magick") {
            log::debug!("[RustyMirror:RS] magick found in PATH");
            return Some(PathBuf::from("magick"));
        }
        log::debug!("[RustyMirror:RS] magick NOT found");
        None
    }
    #[cfg(target_os = "macos")]
    { Some(PathBuf::from("sips")) }
    #[cfg(target_os = "linux")]
    {
        if which_exists("convert") { return Some(PathBuf::from("convert")); }
        if which_exists("magick")  { return Some(PathBuf::from("magick"));  }
        None
    }
}

fn which_exists(cmd: &str) -> bool {
    std::process::Command::new(cmd).arg("--version").output().is_ok()
}

// ── Metadata extraction (no full decode) ─────────────────────────────────────

/// Extract dimensions from HEIC using `magick identify` — much faster than
/// full conversion because it only reads the file header.
pub fn heic_dimensions(path: &Path, resource_dir: Option<&Path>) -> (u32, u32) {
    let cmd = match magick_path(resource_dir) {
        Some(c) => c,
        None    => return (0, 0),
    };

    // `magick identify -format "%wx%h" file.heic` prints e.g. "4032x3024"
    #[cfg(target_os = "windows")]
    let output = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        std::process::Command::new(cmd)
            .args(["identify", "-format", "%wx%h", path.to_str().unwrap_or("")])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
    };

    #[cfg(not(target_os = "windows"))]
    let output = std::process::Command::new(cmd)
        .args(["identify", "-format", "%wx%h", path.to_str().unwrap_or("")])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout);
            let s = s.trim();
            if let Some((w, h)) = s.split_once('x') {
                let w = w.parse::<u32>().unwrap_or(0);
                let h = h.parse::<u32>().unwrap_or(0);
                return (w, h);
            }
            (0, 0)
        }
        _ => (0, 0),
    }
}

// ── Full conversion (only for pHash) ─────────────────────────────────────────

/// Converts a HEIC file to a temporary JPEG.
/// Returns (temp_path, width, height) or None if conversion fails.
pub fn heic_to_temp_jpeg(
    heic_path: &Path,
    resource_dir: Option<&Path>,
) -> Option<(PathBuf, u32, u32)> {
    let stem = heic_path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "heic_tmp".to_string());
    let tmp = std::env::temp_dir().join(format!("rustymirror_{}.jpg", stem));

    convert_one(heic_path, &tmp, resource_dir).ok()?;
    if !tmp.exists() { return None; }

    let (w, h) = image::image_dimensions(&tmp).unwrap_or((0, 0));
    Some((tmp, w, h))
}

fn convert_one(input: &Path, output: &Path, resource_dir: Option<&Path>) -> Result<()> {
    let cmd = magick_path(resource_dir).context("no HEIC converter")?;

    #[cfg(target_os = "macos")]
    let status = std::process::Command::new(cmd)
        .args([input.to_str().unwrap(), "--setProperty", "format", "jpeg",
               "--out", output.to_str().unwrap()])
        .status().context("sips failed")?;

    #[cfg(target_os = "windows")]
    let status = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        std::process::Command::new(cmd)
            .args([input.to_str().unwrap(), output.to_str().unwrap()])
            .creation_flags(CREATE_NO_WINDOW)
            .status().context("magick failed")?
    };

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let status = std::process::Command::new(cmd)
        .args([input.to_str().unwrap(), output.to_str().unwrap()])
        .status().context("magick failed")?;

    if status.success() { Ok(()) } else { anyhow::bail!("converter exit {}", status) }
}

/// Batch-converts HEIC files in parallel. Returns (original, temp, w, h).
pub fn batch_convert_heic(
    heic_paths: &[PathBuf],
    resource_dir: Option<&Path>,
    progress_cb: impl Fn(usize, usize) + Send + Sync,
) -> Vec<(PathBuf, PathBuf, u32, u32)> {
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Resolve magick once before parallel work
    let _ = magick_path(resource_dir);

    let total   = heic_paths.len();
    let counter = AtomicUsize::new(0);

    heic_paths.par_iter().filter_map(|src| {
        let result = heic_to_temp_jpeg(src, resource_dir);
        let done = counter.fetch_add(1, Ordering::Relaxed) + 1;
        progress_cb(done, total);
        result.map(|(dst, w, h)| (src.clone(), dst, w, h))
    }).collect()
}

pub fn cleanup_temp(path: &Path) {
    let _ = std::fs::remove_file(path);
}
