//! HEIC support: metadata extraction and JPEG conversion via magick.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use anyhow::{Context, Result};

// ── Cached magick path ────────────────────────────────────────────────────────
static MAGICK_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

pub fn magick_path(resource_dir: Option<&Path>) -> Option<&'static PathBuf> {
    MAGICK_PATH.get_or_init(|| find_magick(resource_dir)).as_ref()
}

fn find_magick(_resource_dir: Option<&Path>) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(res) = _resource_dir {
            // Dev mode: resource_dir == src-tauri/, magick.exe is in resources/ subdir
            candidates.push(res.join("resources").join("magick.exe"));
            // Direct fallback
            candidates.push(res.join("magick.exe"));
            // Walk up two levels (target/debug/ → src-tauri/resources/)
            if let Some(src_tauri) = res.parent().and_then(|p| p.parent()) {
                candidates.push(src_tauri.join("resources").join("magick.exe"));
            }
        }
        // Relative CWD fallback (project root as CWD)
        candidates.push(PathBuf::from("src-tauri").join("resources").join("magick.exe"));
        // Fallback: look next to the current executable (works in dev and release)
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                candidates.push(exe_dir.join("resources").join("magick.exe"));
                candidates.push(exe_dir.join("magick.exe"));
            }
        }

        for candidate in &candidates {
            tracing::debug!("checking magick at: {}", candidate.display());
            if candidate.exists() {
                tracing::debug!("magick found: {}", candidate.display());
                return Some(candidate.clone());
            }
        }
        if which_exists("magick") {
            tracing::debug!("magick found in PATH");
            return Some(PathBuf::from("magick"));
        }
        tracing::warn!(checked = ?candidates, "magick NOT found");
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

#[cfg(not(target_os = "macos"))]
fn which_exists(cmd: &str) -> bool {
    std::process::Command::new(cmd).arg("--version").output().is_ok()
}

fn stop_requested(stop: Option<&AtomicBool>) -> bool {
    stop.map(|s| s.load(Ordering::Relaxed)).unwrap_or(false)
}

/// Waits for `child` to exit, killing it if it exceeds `timeout` or if `stop`
/// is set. Returns an error on timeout or stop so callers treat it as a failure.
fn wait_timeout(
    mut child: std::process::Child,
    timeout: std::time::Duration,
    stop: Option<&AtomicBool>,
) -> Result<std::process::ExitStatus> {
    let start = std::time::Instant::now();
    loop {
        match child.try_wait()? {
            Some(status) => return Ok(status),
            None if stop_requested(stop) => {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!("HEIC conversion aborted");
            }
            None if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!("HEIC converter timed out after {:?}", timeout);
            }
            None => std::thread::sleep(std::time::Duration::from_millis(100)),
        }
    }
}

const IDENTIFY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Runs a command capturing stdout, killing it if it exceeds `timeout` or if
/// `stop` is set. A background thread drains stdout so a full pipe buffer can
/// never deadlock the polling loop. Returns the captured stdout on a clean exit.
fn output_with_timeout(
    mut cmd: std::process::Command,
    timeout: std::time::Duration,
    stop: Option<&AtomicBool>,
) -> Option<String> {
    use std::process::Stdio;
    use std::io::Read;

    cmd.stdout(Stdio::piped()).stderr(Stdio::null()).stdin(Stdio::null());
    let mut child = cmd.spawn().ok()?;

    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stdout.read_to_string(&mut buf);
        buf
    });

    let start = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if stop_requested(stop) || start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return None;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
            Err(_) => {
                let _ = reader.join();
                return None;
            }
        }
    };

    let out = reader.join().ok()?;
    if status.success() { Some(out) } else { None }
}

// ── Metadata extraction (no full decode) ─────────────────────────────────────

/// Extract dimensions from HEIC using `magick identify` — much faster than
/// full conversion because it only reads the file header.
#[allow(dead_code)]
pub fn heic_dimensions(path: &Path, resource_dir: Option<&Path>, stop: Option<&AtomicBool>) -> (u32, u32) {
    let cmd = match magick_path(resource_dir) {
        Some(c) => c,
        None    => return (0, 0),
    };

    // `magick identify -format "%wx%h" file.heic` prints e.g. "4032x3024"
    #[cfg(target_os = "windows")]
    let command = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let mut c = std::process::Command::new(cmd);
        c.args(["identify", "-format", "%wx%h", path.to_str().unwrap_or("")])
            .creation_flags(CREATE_NO_WINDOW);
        c
    };

    #[cfg(not(target_os = "windows"))]
    let command = {
        let mut c = std::process::Command::new(cmd);
        c.args(["identify", "-format", "%wx%h", path.to_str().unwrap_or("")]);
        c
    };

    match output_with_timeout(command, IDENTIFY_TIMEOUT, stop) {
        Some(s) => {
            let s = s.trim();
            if let Some((w, h)) = s.split_once('x') {
                let w = w.parse::<u32>().unwrap_or(0);
                let h = h.parse::<u32>().unwrap_or(0);
                return (w, h);
            }
            (0, 0)
        }
        None => (0, 0),
    }
}

// ── Full conversion (only for pHash) ─────────────────────────────────────────

/// Converts a HEIC file to a temporary JPEG.
/// `max_dim`: if Some(n), resize so neither dimension exceeds n pixels (for
/// scanner use — pHash only needs a small image; saves ~50× disk I/O vs full-res).
/// Pass None for thumbnail/viewer conversions that need full resolution.
/// Returns (temp_path, width, height) or None if conversion fails.
pub fn heic_to_temp_jpeg(
    heic_path: &Path,
    resource_dir: Option<&Path>,
    max_dim: Option<u32>,
    stop: Option<&AtomicBool>,
) -> Option<(PathBuf, u32, u32)> {
    let stem = heic_path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "heic_tmp".to_string());
    let tmp = std::env::temp_dir().join(format!("rustymirror_{}.jpg", stem));

    // Capture original dimensions before any resize so callers always get the
    // source resolution, not the downscaled temp-file resolution.
    let (w, h) = heic_dimensions(heic_path, resource_dir, stop);

    convert_one(heic_path, &tmp, resource_dir, max_dim, stop).ok()?;
    if !tmp.exists() { return None; }

    Some((tmp, w, h))
}

const CONVERT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

fn convert_one(input: &Path, output: &Path, resource_dir: Option<&Path>, max_dim: Option<u32>, stop: Option<&AtomicBool>) -> Result<()> {
    let cmd = magick_path(resource_dir).context("no HEIC converter")?;

    // `-resize NxN>` shrinks only if either dimension exceeds N; `>` is passed
    // as a literal arg (not a shell redirect) so it is safe in Command::new.
    let resize_arg: Option<String> = max_dim.map(|n| format!("{}x{}>", n, n));

    #[cfg(target_os = "macos")]
    let status = {
        let mut c = std::process::Command::new(cmd);
        if let Some(ref r) = resize_arg {
            // sips uses --resampleHeightWidthMax for proportional downscale
            c.args(["--resampleHeightWidthMax", r.trim_end_matches('>')]);
        }
        c.args([input.to_str().unwrap(), "--setProperty", "format", "jpeg",
                "--out", output.to_str().unwrap()]);
        wait_timeout(c.spawn().context("sips failed")?, CONVERT_TIMEOUT, stop)?
    };

    #[cfg(target_os = "windows")]
    let status = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let mut c = std::process::Command::new(cmd);
        c.arg(input.to_str().unwrap());
        if let Some(ref r) = resize_arg {
            c.args(["-resize", r]);
        }
        c.arg(output.to_str().unwrap())
         .creation_flags(CREATE_NO_WINDOW);
        wait_timeout(c.spawn().context("magick failed")?, CONVERT_TIMEOUT, stop)?
    };

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let status = {
        let mut c = std::process::Command::new(cmd);
        c.arg(input.to_str().unwrap());
        if let Some(ref r) = resize_arg {
            c.args(["-resize", r]);
        }
        c.arg(output.to_str().unwrap());
        wait_timeout(c.spawn().context("magick/convert failed")?, CONVERT_TIMEOUT, stop)?
    };

    if status.success() { Ok(()) } else { anyhow::bail!("converter exit {}", status) }
}


pub fn cleanup_temp(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// Extract pixel dimensions and capture date from a HEIC/HEIF/AVIF file.
///
/// Converts to a small temp JPEG via magick, reads original dimensions from
/// the conversion output, then parses `DateTimeOriginal` from the JPEG's
/// embedded EXIF block (local wall-clock time, not QuickTime UTC).
///
/// Returns `(width, height, capture_date_iso)`.
/// Width/height are 0 and date is `None` if the conversion fails.
pub fn heic_capture_info(path: &Path, resource_dir: Option<&Path>, stop: Option<&AtomicBool>) -> (u32, u32, Option<String>) {
    let Some((tmp_path, w, h)) = heic_to_temp_jpeg(path, resource_dir, Some(512), stop) else {
        return (0, 0, None);
    };
    let date = std::fs::read(&tmp_path)
        .ok()
        .and_then(|bytes| crate::scanner::record::parse_exif_date(&bytes));
    cleanup_temp(&tmp_path);
    (w, h, date)
}

