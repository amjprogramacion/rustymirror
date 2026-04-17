//! ExifTool sidecar — path resolution, single-file and batch JSON I/O.
//!
//! The binary is expected at `resources/exiftool[.exe]` alongside the app
//! (mirrors how `magick.exe` is bundled via `tauri.conf.json` resources glob).

use std::{
    io::Write as _,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

static EXIFTOOL_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

fn which_exists(cmd: &str) -> bool {
    let mut c = Command::new(cmd);
    c.arg("-ver");
    #[cfg(target_os = "windows")]
    c.creation_flags(CREATE_NO_WINDOW);
    c.output().is_ok()
}

/// Resolve the exiftool binary path once per session (same pattern as `find_magick`).
pub fn find_exiftool(resource_dir: &Path) -> Option<PathBuf> {
    EXIFTOOL_PATH
        .get_or_init(|| {
            #[cfg(target_os = "windows")]
            let bin = "exiftool.exe";
            #[cfg(not(target_os = "windows"))]
            let bin = "exiftool";

            let mut candidates: Vec<PathBuf> = Vec::new();

            // Dev mode: resource_dir == src-tauri/, binary is in resources/ subdir
            candidates.push(resource_dir.join("resources").join(bin));
            // Direct fallback next to resource_dir
            candidates.push(resource_dir.join(bin));
            // Walk up two levels (target/debug/ → src-tauri/resources/)
            if let Some(src_tauri) = resource_dir.parent().and_then(|p| p.parent()) {
                candidates.push(src_tauri.join("resources").join(bin));
            }
            // Relative CWD fallback (project root as CWD)
            candidates.push(PathBuf::from("src-tauri").join("resources").join(bin));
            // Next to the current executable
            if let Ok(exe) = std::env::current_exe() {
                if let Some(exe_dir) = exe.parent() {
                    candidates.push(exe_dir.join("resources").join(bin));
                    candidates.push(exe_dir.join(bin));
                }
            }

            for c in &candidates {
                tracing::debug!("checking exiftool at: {}", c.display());
                if c.exists() {
                    tracing::debug!("exiftool found: {}", c.display());
                    return Some(c.clone());
                }
            }

            if which_exists("exiftool") {
                tracing::debug!("exiftool found in PATH");
                return Some(PathBuf::from("exiftool"));
            }

            tracing::warn!(checked = ?candidates, "exiftool NOT found");
            None
        })
        .clone()
}

/// Run `exiftool -json <extra_args> <path>` and return the first JSON object.
///
/// Fields suffixed with `#` in `extra_args` bypass PrintConv (return raw numbers).
/// GPS tags should always use `#` so coordinates come back as decimal f64.
pub fn read_tags(
    exiftool: &Path,
    image_path: &Path,
    extra_args: &[&str],
) -> anyhow::Result<serde_json::Value> {
    let output = base_cmd(exiftool)
        .arg("-json")
        .args(extra_args)
        .arg(image_path)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("exiftool read failed: {stderr}");
    }

    parse_first_obj(&output.stdout)
}

/// Run exiftool on a batch of files via a temp argfile.
///
/// Returns one JSON object per file in the same order as `paths`.
/// The `extra_args` list is passed before `-@ argfile` so tags apply to all files.
pub fn batch_read_tags(
    exiftool: &Path,
    paths: &[PathBuf],
    extra_args: &[&str],
) -> anyhow::Result<Vec<serde_json::Value>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    // Write one path per line to a temp argfile.
    let argfile_path = std::env::temp_dir()
        .join(format!("rustymirror_et_{}.txt", std::process::id()));

    let mut f = std::fs::File::create(&argfile_path)?;
    for p in paths {
        writeln!(f, "{}", p.to_string_lossy())?;
    }
    drop(f);

    let output = base_cmd(exiftool)
        .arg("-json")
        .args(extra_args)
        .arg("-@")
        .arg(&argfile_path)
        .output();

    let _ = std::fs::remove_file(&argfile_path);
    let output = output?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("exiftool batch read failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return Ok(Vec::new());
    }
    let arr: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or(serde_json::Value::Array(Vec::new()));
    Ok(arr.as_array().cloned().unwrap_or_default())
}

/// Apply `tag=value` pairs to a file in-place (`-overwrite_original`).
pub fn write_tags(
    exiftool: &Path,
    image_path: &Path,
    tags: &[(&str, String)],
) -> anyhow::Result<()> {
    if tags.is_empty() {
        return Ok(());
    }

    let mut cmd = base_cmd(exiftool);
    cmd.arg("-overwrite_original");
    for (tag, value) in tags {
        cmd.arg(format!("-{tag}={value}"));
    }
    cmd.arg(image_path);

    let output = cmd.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("exiftool write failed: {stderr}");
    }
    Ok(())
}

// ── internal helpers ─────────────────────────────────────────────────────────

fn parse_first_obj(stdout: &[u8]) -> anyhow::Result<serde_json::Value> {
    let text = String::from_utf8_lossy(stdout);
    if text.trim().is_empty() {
        return Ok(serde_json::Value::Object(Default::default()));
    }
    let mut arr: serde_json::Value = serde_json::from_str(&text)?;
    let obj = arr
        .as_array_mut()
        .and_then(|a| if a.is_empty() { None } else { Some(a.remove(0)) })
        .unwrap_or(serde_json::Value::Object(Default::default()));
    Ok(obj)
}

fn base_cmd(exiftool: &Path) -> Command {
    let mut cmd = Command::new(exiftool);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}
