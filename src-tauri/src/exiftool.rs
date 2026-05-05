//! ExifTool sidecar — path resolution, single-file and batch JSON I/O.
//!
//! The binary is expected at `resources/exiftool[.exe]` alongside the app
//! (mirrors how `magick.exe` is bundled via `tauri.conf.json` resources glob).

use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{Arc, Mutex, OnceLock},
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
///
/// The file path is written to a temp argfile (UTF-8) so non-ASCII characters in
/// network-drive paths (e.g. accented letters) are handled correctly on Windows.
pub fn read_tags(
    exiftool: &Path,
    image_path: &Path,
    extra_args: &[&str],
) -> anyhow::Result<serde_json::Value> {
    let argfile_path = std::env::temp_dir()
        .join(format!("rustymirror_et_read_{}.txt", std::process::id()));
    {
        let mut f = std::fs::File::create(&argfile_path)?;
        writeln!(f, "{}", image_path.to_string_lossy())?;
    }

    let result = base_cmd(exiftool)
        .arg("-json")
        .args(extra_args)
        .arg("-@")
        .arg(&argfile_path)
        .output();

    let _ = std::fs::remove_file(&argfile_path);
    let output = result?;

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
///
/// The file path is written to a temp argfile (UTF-8) so non-ASCII characters in
/// network-drive paths (e.g. accented letters) are handled correctly on Windows.
pub fn write_tags(
    exiftool: &Path,
    image_path: &Path,
    tags: &[(&str, String)],
) -> anyhow::Result<()> {
    if tags.is_empty() {
        return Ok(());
    }

    let argfile_path = std::env::temp_dir()
        .join(format!("rustymirror_et_write_{}.txt", std::process::id()));
    {
        let mut f = std::fs::File::create(&argfile_path)?;
        writeln!(f, "{}", image_path.to_string_lossy())?;
    }

    let mut cmd = base_cmd(exiftool);
    cmd.arg("-overwrite_original");
    for (tag, value) in tags {
        cmd.arg(format!("-{tag}={value}"));
    }
    cmd.arg("-@").arg(&argfile_path);

    let result = cmd.output();
    let _ = std::fs::remove_file(&argfile_path);
    let output = result?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    tracing::debug!("exiftool write stdout: {stdout}");
    if !stderr.is_empty() {
        tracing::warn!("exiftool write stderr: {stderr}");
    }
    if !output.status.success() {
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
    // Tell ExifTool that filenames (passed via argfile or CLI) are UTF-8 encoded.
    // Required for non-ASCII paths (accented chars, CJK, etc.) on Windows.
    cmd.args(["-charset", "FileName=UTF8"]);
    cmd
}

// ── ExifTool daemon (stay-open mode) ─────────────────────────────────────────

/// A long-lived ExifTool process that accepts multiple batch queries over stdin/stdout.
///
/// Eliminates per-query Perl startup overhead: the process is spawned once and
/// kept alive for the duration of the scan.  Each call to `batch_query` sends
/// the file list + tags via stdin, waits for the `{ready}` sentinel on stdout,
/// and returns the parsed JSON array.
///
/// Stdin is written from a background thread so that large outputs on stdout
/// cannot fill the OS pipe buffer and deadlock the call.
///
/// On Windows the child is assigned to a Job Object with KILL_ON_JOB_CLOSE so
/// that the OS terminates it automatically if the parent process dies before
/// `Drop` can run the graceful shutdown.
pub struct ExifToolDaemon {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    stdout: BufReader<ChildStdout>,
    #[cfg(target_os = "windows")]
    job_handle: usize, // stores HANDLE (*mut c_void) as integer — keeps the struct Send
}

impl ExifToolDaemon {
    pub fn start(exiftool: &Path) -> anyhow::Result<Self> {
        let mut child = base_cmd(exiftool)
            .args(["-stay_open", "True", "-@", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        #[cfg(target_os = "windows")]
        let job_handle = Self::attach_job(&child);

        let stdin = Arc::new(Mutex::new(
            child.stdin.take().ok_or_else(|| anyhow::anyhow!("no stdin"))?,
        ));
        let stdout = BufReader::new(
            child.stdout.take().ok_or_else(|| anyhow::anyhow!("no stdout"))?,
        );
        Ok(Self {
            child,
            stdin,
            stdout,
            #[cfg(target_os = "windows")]
            job_handle,
        })
    }

    /// Creates a Windows Job Object with KILL_ON_JOB_CLOSE and assigns `child` to it.
    /// Returns the raw job HANDLE (0 on failure — errors are non-fatal).
    #[cfg(target_os = "windows")]
    fn attach_job(child: &Child) -> usize {
        use core::ffi::c_void;
        use std::os::windows::io::AsRawHandle;

        const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x00002000;
        const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS: u32 = 9;

        #[repr(C)]
        struct BasicLimitInfo {
            per_process_user_time_limit: i64,
            per_job_user_time_limit: i64,
            limit_flags: u32,
            minimum_working_set_size: usize,
            maximum_working_set_size: usize,
            active_process_limit: u32,
            affinity: usize,
            priority_class: u32,
            scheduling_class: u32,
        }

        #[repr(C)]
        struct IoCounters { read_ops: u64, write_ops: u64, other_ops: u64, read_bytes: u64, write_bytes: u64, other_bytes: u64 }

        #[repr(C)]
        struct ExtendedLimitInfo {
            basic: BasicLimitInfo,
            io: IoCounters,
            process_memory_limit: usize,
            job_memory_limit: usize,
            peak_process_memory: usize,
            peak_job_memory: usize,
        }

        #[link(name = "kernel32")]
        extern "system" {
            fn CreateJobObjectW(attrs: *const c_void, name: *const u16) -> *mut c_void;
            fn SetInformationJobObject(job: *mut c_void, class: u32, info: *const c_void, len: u32) -> i32;
            fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
            fn CloseHandle(handle: *mut c_void) -> i32;
        }

        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() { return 0; }

            let mut info = std::mem::zeroed::<ExtendedLimitInfo>();
            info.basic.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS,
                &info as *const _ as *const c_void,
                std::mem::size_of::<ExtendedLimitInfo>() as u32,
            ) == 0 {
                CloseHandle(job);
                return 0;
            }

            let child_handle = child.as_raw_handle();
            if AssignProcessToJobObject(job, child_handle) == 0 {
                CloseHandle(job);
                return 0;
            }

            job as usize
        }
    }

    /// Send `paths` + `tags` to the daemon and return one JSON object per file.
    pub fn batch_query(
        &mut self,
        paths: &[std::path::PathBuf],
        tags: &[&str],
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }

        // Build the full command block as a single string.
        let mut cmd = String::with_capacity(paths.len() * 64 + tags.len() * 24 + 16);
        cmd.push_str("-json\n");
        for &tag in tags {
            cmd.push_str(tag);
            cmd.push('\n');
        }
        for p in paths {
            cmd.push_str(&p.to_string_lossy());
            cmd.push('\n');
        }
        cmd.push_str("-execute\n");

        // Write stdin in a background thread so large stdout output cannot fill
        // the OS pipe buffer (64 KB on Linux) and deadlock the caller.
        let stdin = Arc::clone(&self.stdin);
        let writer = std::thread::spawn(move || -> std::io::Result<()> {
            let mut guard = stdin.lock().unwrap();
            guard.write_all(cmd.as_bytes())?;
            guard.flush()
        });

        // Read stdout until the `{ready}` sentinel.
        let mut output = String::new();
        let mut line = String::new();
        loop {
            line.clear();
            if self.stdout.read_line(&mut line)? == 0 {
                break; // EOF — process exited unexpectedly
            }
            if line.trim_end().starts_with("{ready}") {
                break;
            }
            output.push_str(&line);
        }

        writer
            .join()
            .map_err(|_| anyhow::anyhow!("stdin writer thread panicked"))?
            .map_err(|e| anyhow::anyhow!("stdin write error: {e}"))?;

        if output.trim().is_empty() {
            return Ok(Vec::new());
        }
        let arr: serde_json::Value = serde_json::from_str(&output)
            .unwrap_or(serde_json::Value::Array(Vec::new()));
        Ok(arr.as_array().cloned().unwrap_or_default())
    }
}

impl Drop for ExifToolDaemon {
    fn drop(&mut self) {
        // Graceful shutdown: ask ExifTool to exit, then wait.
        if let Ok(mut guard) = self.stdin.lock() {
            let _ = write!(guard, "-stay_open\nFalse\n");
            let _ = guard.flush();
        }
        let _ = self.child.wait();

        // Close the Job Object handle. If Drop never ran (parent crash/kill),
        // the OS closes it automatically on process exit, killing the child.
        #[cfg(target_os = "windows")]
        if self.job_handle != 0 {
            #[link(name = "kernel32")]
            extern "system" { fn CloseHandle(h: *mut core::ffi::c_void) -> i32; }
            unsafe { CloseHandle(self.job_handle as *mut _); }
        }
    }
}
