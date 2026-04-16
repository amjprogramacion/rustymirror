//! HEIC support: metadata extraction and JPEG conversion via magick.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use anyhow::{Context, Result, anyhow};

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

fn which_exists(cmd: &str) -> bool {
    std::process::Command::new(cmd).arg("--version").output().is_ok()
}

/// Waits for `child` to exit, killing it and returning an error if it exceeds `timeout`.
fn wait_timeout(mut child: std::process::Child, timeout: std::time::Duration) -> Result<std::process::ExitStatus> {
    let start = std::time::Instant::now();
    loop {
        match child.try_wait()? {
            Some(status) => return Ok(status),
            None if start.elapsed() >= timeout => {
                let _ = child.kill();
                anyhow::bail!("HEIC converter timed out after {:?}", timeout);
            }
            None => std::thread::sleep(std::time::Duration::from_millis(100)),
        }
    }
}

// ── Metadata extraction (no full decode) ─────────────────────────────────────

/// Extract dimensions from HEIC using `magick identify` — much faster than
/// full conversion because it only reads the file header.
#[allow(dead_code)]
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
/// `max_dim`: if Some(n), resize so neither dimension exceeds n pixels (for
/// scanner use — pHash only needs a small image; saves ~50× disk I/O vs full-res).
/// Pass None for thumbnail/viewer conversions that need full resolution.
/// Returns (temp_path, width, height) or None if conversion fails.
pub fn heic_to_temp_jpeg(
    heic_path: &Path,
    resource_dir: Option<&Path>,
    max_dim: Option<u32>,
) -> Option<(PathBuf, u32, u32)> {
    let stem = heic_path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "heic_tmp".to_string());
    let tmp = std::env::temp_dir().join(format!("rustymirror_{}.jpg", stem));

    // Capture original dimensions before any resize so callers always get the
    // source resolution, not the downscaled temp-file resolution.
    let (w, h) = heic_dimensions(heic_path, resource_dir);

    convert_one(heic_path, &tmp, resource_dir, max_dim).ok()?;
    if !tmp.exists() { return None; }

    Some((tmp, w, h))
}

const CONVERT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

fn convert_one(input: &Path, output: &Path, resource_dir: Option<&Path>, max_dim: Option<u32>) -> Result<()> {
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
        wait_timeout(c.spawn().context("sips failed")?, CONVERT_TIMEOUT)?
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
        wait_timeout(c.spawn().context("magick failed")?, CONVERT_TIMEOUT)?
    };

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let status = {
        let mut c = std::process::Command::new(cmd);
        c.arg(input.to_str().unwrap());
        if let Some(ref r) = resize_arg {
            c.args(["-resize", r]);
        }
        c.arg(output.to_str().unwrap());
        wait_timeout(c.spawn().context("magick/convert failed")?, CONVERT_TIMEOUT)?
    };

    if status.success() { Ok(()) } else { anyhow::bail!("converter exit {}", status) }
}

/// Batch-converts HEIC files in parallel. Returns (original, temp, w, h).
/// `max_dim`: if Some(n), outputs are resized to at most n×n pixels (scanner
/// use case — pHash quality is identical but temp files are ~50× smaller).
pub fn batch_convert_heic(
    heic_paths: &[PathBuf],
    resource_dir: Option<&Path>,
    max_dim: Option<u32>,
    progress_cb: impl Fn(usize, usize) + Send + Sync,
) -> Vec<(PathBuf, PathBuf, u32, u32)> {
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Resolve magick once before parallel work
    let _ = magick_path(resource_dir);

    let total   = heic_paths.len();
    let counter = AtomicUsize::new(0);

    heic_paths.par_iter().filter_map(|src| {
        let result = heic_to_temp_jpeg(src, resource_dir, max_dim);
        let done = counter.fetch_add(1, Ordering::Relaxed) + 1;
        progress_cb(done, total);
        result.map(|(dst, w, h)| (src.clone(), dst, w, h))
    }).collect()
}

pub fn cleanup_temp(path: &Path) {
    let _ = std::fs::remove_file(path);
}

// ── ISOBMFF / HEIC EXIF locator ──────────────────────────────────────────────

/// Returns `(file_byte_offset, byte_length)` of the raw EXIF item payload
/// (including the mandatory 4-byte `tiff_header_offset` prefix) in a HEIC/HEIF
/// (or AVIF) file.  Reads only box headers — no pixel data is decoded.
pub fn find_exif_in_heic(data: &[u8]) -> Result<(usize, usize)> {
    let (meta_start, meta_end) = find_meta_children(data)?;
    let meta = &data[meta_start..meta_end];
    let exif_id = find_exif_item_id(meta)?;
    find_item_in_iloc(data, meta, exif_id)
}

// ── Low-level box helpers ─────────────────────────────────────────────────────

/// Returns `(total_box_size_in_bytes, header_length)` for the box at `pos`.
fn parse_box_size(data: &[u8], pos: usize) -> Result<(usize, usize)> {
    if pos + 8 > data.len() {
        return Err(anyhow!("ISOBMFF box header truncated at offset {pos}"));
    }
    let size32 = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap());
    match size32 {
        0 => Ok((data.len() - pos, 8)), // box extends to EOF
        1 => {
            // 64-bit extended size in the next 8 bytes
            if pos + 16 > data.len() {
                return Err(anyhow!("ISOBMFF extended size truncated at {pos}"));
            }
            let size64 = u64::from_be_bytes(data[pos + 8..pos + 16].try_into().unwrap()) as usize;
            Ok((size64, 16))
        }
        n => Ok((n as usize, 8)),
    }
}

fn read_u16_be(data: &[u8], pos: usize) -> Result<u16> {
    data.get(pos..pos + 2)
        .and_then(|b| b.try_into().ok())
        .map(u16::from_be_bytes)
        .ok_or_else(|| anyhow!("read_u16_be out of range at {pos}"))
}

fn read_u32_be(data: &[u8], pos: usize) -> Result<u32> {
    data.get(pos..pos + 4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_be_bytes)
        .ok_or_else(|| anyhow!("read_u32_be out of range at {pos}"))
}

/// Reads a big-endian unsigned integer of `size` bytes (0–8).  Returns 0 for
/// `size == 0` without touching `data`.
fn read_uint_be(data: &[u8], pos: usize, size: usize) -> Result<u64> {
    if size == 0 {
        return Ok(0);
    }
    data.get(pos..pos + size)
        .ok_or_else(|| anyhow!("read_uint_be out of range at {pos}+{size}"))
        .map(|bytes| bytes.iter().fold(0u64, |acc, &b| (acc << 8) | b as u64))
}

// ── Box walkers ───────────────────────────────────────────────────────────────

/// Walks top-level boxes and returns the byte range of the children inside
/// `meta` (after the FullBox version+flags prefix).
///
/// Searches recursively up to `MAX_DEPTH` levels deep to handle recovered or
/// re-encoded HEIC files that nest `meta` inside `moov`, `udta`, etc.
fn find_meta_children(data: &[u8]) -> Result<(usize, usize)> {
    const MAX_DEPTH: usize = 5;
    let mut top_level_types: Vec<String> = Vec::new();

    // Collect top-level box types for a useful error message.
    {
        let mut pos = 0;
        while pos + 8 <= data.len() {
            if let Ok((box_size, _)) = parse_box_size(data, pos) {
                if box_size == 0 { break; }
                if let Ok(s) = std::str::from_utf8(&data[pos + 4..pos + 8]) {
                    top_level_types.push(s.to_string());
                }
                pos += box_size;
            } else {
                break;
            }
        }
    }

    if let Some(found) = find_meta_recursive(data, data, MAX_DEPTH) {
        return Ok(found);
    }

    Err(anyhow!(
        "No 'meta' box found in HEIC/HEIF file (top-level boxes: [{}])",
        top_level_types.join(", ")
    ))
}

/// Recursively search `boxes` (a sub-slice of `file_data`) for a `meta` FullBox.
/// Returns absolute offsets into `file_data`.
fn find_meta_recursive(boxes: &[u8], file_data: &[u8], depth: usize) -> Option<(usize, usize)> {
    if depth == 0 { return None; }
    let base = boxes.as_ptr() as usize - file_data.as_ptr() as usize;
    let mut pos = 0;
    while pos + 8 <= boxes.len() {
        let (box_size, header_len) = parse_box_size(boxes, pos).ok()?;
        if box_size == 0 { break; }
        if pos + box_size > boxes.len() { break; }

        let box_type = &boxes[pos + 4..pos + 8];

        if box_type == b"meta" {
            // `meta` is a FullBox — 4 bytes of version+flags precede the children.
            let children_start = base + pos + header_len + 4;
            let children_end   = (base + pos + box_size).min(file_data.len());
            return Some((children_start, children_end));
        }

        // Recurse into known container boxes.
        if matches!(box_type, b"moov" | b"udta" | b"trak" | b"mdia" | b"minf" | b"dinf") {
            let inner_start = pos + header_len;
            let inner_end   = pos + box_size;
            if inner_start < inner_end && inner_end <= boxes.len() {
                if let Some(found) = find_meta_recursive(
                    &boxes[inner_start..inner_end],
                    file_data,
                    depth - 1,
                ) {
                    return Some(found);
                }
            }
        }

        pos += box_size;
    }
    None
}

/// Walks `iinf` inside `meta_children` and returns the item_id of the first
/// entry whose `item_type` is `"Exif"` (requires `infe` version ≥ 2).
fn find_exif_item_id(meta_children: &[u8]) -> Result<u32> {
    let mut pos = 0;
    while pos + 8 <= meta_children.len() {
        let (box_size, header_len) = parse_box_size(meta_children, pos)?;
        if box_size == 0 {
            break;
        }
        if &meta_children[pos + 4..pos + 8] == b"iinf" {
            // FullBox: skip version(1) + flags(3)
            let full_start = pos + header_len;
            if full_start + 4 > meta_children.len() {
                return Err(anyhow!("'iinf' box too small"));
            }
            let iinf_version = meta_children[full_start];
            let mut p = full_start + 4;

            let (entry_count, p2) = if iinf_version == 0 {
                (read_u16_be(meta_children, p)? as u32, p + 2)
            } else {
                (read_u32_be(meta_children, p)?, p + 4)
            };
            p = p2;

            for _ in 0..entry_count {
                if p + 8 > meta_children.len() {
                    break;
                }
                let (entry_size, entry_hdr) = parse_box_size(meta_children, p)?;
                if entry_size == 0 {
                    break;
                }
                if &meta_children[p + 4..p + 8] == b"infe" {
                    let infe_full = p + entry_hdr;
                    if infe_full + 4 > meta_children.len() {
                        break;
                    }
                    let infe_ver = meta_children[infe_full];
                    let mut d = infe_full + 4; // skip version+flags

                    // item_type field only exists in infe version ≥ 2.
                    if infe_ver >= 2 {
                        let (item_id, after_id) = if infe_ver == 2 {
                            (read_u16_be(meta_children, d)? as u32, d + 2)
                        } else {
                            // version 3+: 4-byte item_id
                            (read_u32_be(meta_children, d)?, d + 4)
                        };
                        d = after_id + 2; // skip protection_index (2 bytes)
                        if d + 4 <= meta_children.len()
                            && &meta_children[d..d + 4] == b"Exif"
                        {
                            return Ok(item_id);
                        }
                    }
                }
                p += entry_size;
            }
        }
        pos += box_size;
    }
    Err(anyhow!("No EXIF item found in HEIC 'iinf' box"))
}

/// Parses the `iloc` box inside `meta_children` and returns the absolute file
/// offset and byte length of the item with `target_id`.
///
/// Only supports single-extent, file-based items (construction_method == 0),
/// which covers all camera-produced HEIC files.
fn find_item_in_iloc(
    file_data: &[u8],
    meta_children: &[u8],
    target_id: u32,
) -> Result<(usize, usize)> {
    let mut pos = 0;
    while pos + 8 <= meta_children.len() {
        let (box_size, header_len) = parse_box_size(meta_children, pos)?;
        if box_size == 0 {
            break;
        }
        if &meta_children[pos + 4..pos + 8] == b"iloc" {
            // FullBox: skip version(1) + flags(3)
            let full_start = pos + header_len;
            if full_start + 4 > meta_children.len() {
                return Err(anyhow!("'iloc' box too small"));
            }
            let version = meta_children[full_start];
            let mut p = full_start + 4;

            // Byte: (offset_size << 4) | length_size
            let b1 = *meta_children.get(p).ok_or_else(|| anyhow!("iloc truncated"))?;
            p += 1;
            let offset_size = ((b1 >> 4) & 0xF) as usize;
            let length_size = (b1 & 0xF) as usize;

            // Byte: (base_offset_size << 4) | (index_size if version ≥ 1, else 0)
            let b2 = *meta_children.get(p).ok_or_else(|| anyhow!("iloc truncated"))?;
            p += 1;
            let base_offset_size = ((b2 >> 4) & 0xF) as usize;
            let index_size = if version >= 1 { (b2 & 0xF) as usize } else { 0 };

            let (item_count, p2) = if version < 2 {
                (read_u16_be(meta_children, p)? as u32, p + 2)
            } else {
                (read_u32_be(meta_children, p)?, p + 4)
            };
            p = p2;

            for _ in 0..item_count {
                // item_id
                let (item_id, p2) = if version < 2 {
                    (read_u16_be(meta_children, p)? as u32, p + 2)
                } else {
                    (read_u32_be(meta_children, p)?, p + 4)
                };
                p = p2;

                // construction_method (only in iloc version ≥ 1)
                let construction_method = if version >= 1 {
                    let cm = read_u16_be(meta_children, p)?;
                    p += 2;
                    cm
                } else {
                    0u16
                };

                p += 2; // data_reference_index

                let base_offset = read_uint_be(meta_children, p, base_offset_size)?;
                p += base_offset_size;

                let extent_count = read_u16_be(meta_children, p)?;
                p += 2;

                // Consume all extents (advancing p), recording the first one if this
                // is our target item.
                let mut first_offset = 0usize;
                let mut first_length = 0usize;
                for i in 0..extent_count {
                    if version >= 1 && index_size > 0 {
                        p += index_size; // extent_index
                    }
                    let ext_offset = read_uint_be(meta_children, p, offset_size)?;
                    p += offset_size;
                    let ext_length = read_uint_be(meta_children, p, length_size)?;
                    p += length_size;
                    if item_id == target_id && i == 0 {
                        first_offset = (base_offset + ext_offset) as usize;
                        first_length = ext_length as usize;
                    }
                }

                if item_id == target_id {
                    if construction_method != 0 {
                        return Err(anyhow!(
                            "HEIC EXIF item uses construction_method={construction_method} \
                             (idat/item-based); only file-based (0) is supported"
                        ));
                    }
                    if extent_count != 1 {
                        return Err(anyhow!(
                            "HEIC EXIF item has {extent_count} extents; \
                             only single-extent items are supported"
                        ));
                    }
                    if first_offset + first_length > file_data.len() {
                        return Err(anyhow!("HEIC EXIF item extends past end of file"));
                    }
                    return Ok((first_offset, first_length));
                }
            }
        }
        pos += box_size;
    }
    Err(anyhow!("EXIF item not found in 'iloc' box"))
}
