use std::collections::HashMap;
use std::path::Path;
use unicode_normalization::UnicodeNormalization;

use crate::cache::CachedFile;
use crate::hasher::{perceptual_hash_from_bytes, read_file_data};
use crate::types::{FailedFileKind, ImageEntry};

use super::walk::is_heic;

pub(super) struct FileRecord {
    pub(super) entry:       ImageEntry,
    pub(super) ex_hash:     String,
    pub(super) ts_tag:      Option<String>,
    pub(super) ph:          Option<image_hasher::ImageHash>,
    pub(super) mtime_key:   String,
    pub(super) header_hash: Option<String>,
}

pub(super) struct HeicExtra {
    pub(super) ph:       image_hasher::ImageHash,
    pub(super) width:    u32,
    pub(super) height:   u32,
    pub(super) modified: String,
}

/// Normalises a path to a stable cache key that survives cold/warm SMB traversal
/// differences: first apply Unicode NFC normalisation (fixes accented characters
/// like `GALERÍA` returning as NFD vs NFC across traversals), then lowercase
/// (fixes drive-letter / folder-name casing differences).
#[inline]
pub(super) fn cache_key(path_str: &str) -> String {
    path_str.nfc().collect::<String>().to_lowercase()
}

/// Blake3 hash of the first 4096 bytes of a file.
/// Used as a cheap, mtime-independent cache validity check.
/// Uses take+read_to_end to guarantee reading up to 4096 bytes even on SMB/NAS,
/// where a single read() call may return fewer bytes than requested.
pub(super) fn read_header_hash(path: &Path) -> Option<String> {
    use std::io::{Read, BufReader};
    let f = std::fs::File::open(path).ok()?;
    let mut buf = Vec::with_capacity(4096);
    BufReader::new(f).take(4096).read_to_end(&mut buf).ok()?;
    Some(blake3::hash(&buf).to_hex().to_string())
}

/// Convert file mtime metadata to an RFC3339 string, falling back to the Unix epoch.
pub(super) fn mtime_rfc3339(meta: &std::fs::Metadata) -> String {
    meta.modified().ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| chrono::DateTime::<chrono::Utc>::from_timestamp(d.as_secs() as i64, 0))
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}

/// Parse the DateTimeOriginal EXIF field from raw image bytes into an RFC3339 string.
///
/// Supports JPEG (APP1/EXIF) and raw TIFF.  Other container formats (PNG, WebP)
/// fall back to the caller's `mtime_rfc3339` fallback, which is acceptable for
/// the duplicate-detection pipeline.
pub(crate) fn parse_exif_date(bytes: &[u8]) -> Option<String> {
    let tiff = jpeg_exif_tiff(bytes).or_else(|| tiff_direct(bytes))?;
    tiff_date_original(tiff)
}

/// Return the TIFF data slice embedded in a JPEG APP1/EXIF segment.
fn jpeg_exif_tiff(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut pos = 2usize;
    while pos + 4 <= bytes.len() {
        if bytes[pos] != 0xFF { break; }
        let marker = bytes[pos + 1];
        if matches!(marker, 0xD8 | 0xD9 | 0xD0..=0xD7) { pos += 2; continue; }
        if pos + 4 > bytes.len() { break; }
        let seg_len = u16::from_be_bytes([bytes[pos + 2], bytes[pos + 3]]) as usize;
        if seg_len < 2 { break; }
        if marker == 0xE1 && seg_len >= 8 {
            let ds = pos + 4;
            if ds + 6 <= bytes.len() && &bytes[ds..ds + 6] == b"Exif\0\0" {
                return Some(&bytes[ds + 6..]);
            }
        }
        if marker == 0xDA { break; }
        pos = pos + 2 + seg_len;
    }
    None
}

/// Return the slice as-is if it starts with a TIFF magic header.
fn tiff_direct(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.len() >= 8 && (bytes.starts_with(b"II") || bytes.starts_with(b"MM")) {
        Some(bytes)
    } else {
        None
    }
}

/// Walk a TIFF byte slice for DateTimeOriginal (tag 0x9003) via IFD0 → ExifIFD.
fn tiff_date_original(tiff: &[u8]) -> Option<String> {
    if tiff.len() < 8 { return None; }
    let le = &tiff[0..2] == b"II";
    let u16_at = |pos: usize| -> Option<u16> {
        let b: [u8; 2] = tiff.get(pos..pos + 2)?.try_into().ok()?;
        Some(if le { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) })
    };
    let u32_at = |pos: usize| -> Option<u32> {
        let b: [u8; 4] = tiff.get(pos..pos + 4)?.try_into().ok()?;
        Some(if le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) })
    };
    if u16_at(2)? != 0x002A { return None; }
    let ifd0 = u32_at(4)? as usize;

    // Scan IFD0 for ExifIFD pointer (0x8769)
    let n0 = u16_at(ifd0)? as usize;
    let mut exif_ifd: Option<usize> = None;
    for i in 0..n0 {
        let e = ifd0 + 2 + i * 12;
        if e + 12 > tiff.len() { break; }
        if u16_at(e)? == 0x8769 {
            exif_ifd = Some(u32_at(e + 8)? as usize);
            break;
        }
    }
    let exif_ifd = exif_ifd?;

    // Scan ExifIFD for DateTimeOriginal (0x9003)
    let nx = u16_at(exif_ifd)? as usize;
    for i in 0..nx {
        let e = exif_ifd + 2 + i * 12;
        if e + 12 > tiff.len() { break; }
        if u16_at(e)? == 0x9003 {
            let count = u32_at(e + 4)? as usize;
            let offset = u32_at(e + 8)? as usize;
            if count >= 19 && offset + count <= tiff.len() {
                let s = std::str::from_utf8(&tiff[offset..offset + 19]).ok()?;
                let b = s.as_bytes();
                if b[4] == b':' && b[7] == b':' && b[10] == b' ' {
                    return Some(format!(
                        "{}-{}-{}T{}:{}:{}Z",
                        &s[0..4], &s[5..7], &s[8..10],
                        &s[11..13], &s[14..16], &s[17..19]
                    ));
                }
            }
        }
    }
    None
}

pub(super) fn extract_timestamp_tag(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_string_lossy().to_string();
    let b = stem.as_bytes();
    for i in 0..b.len().saturating_sub(14) {
        if b[i..i+8].iter().all(|c| c.is_ascii_digit())
            && b[i+8] == b'_'
            && b[i+9..i+15].iter().all(|c| c.is_ascii_digit())
        {
            return Some(stem[i..i+15].to_string());
        }
    }
    None
}

pub(super) fn read_capture_date(path: &Path, bytes: &[u8], meta: &std::fs::Metadata) -> String {
    if let Some(ts) = extract_timestamp_tag(path) {
        if ts.len() == 15 {
            return format!("{}-{}-{}T{}:{}:{}Z",
                &ts[0..4], &ts[4..6], &ts[6..8],
                &ts[9..11], &ts[11..13], &ts[13..15]);
        }
    }
    parse_exif_date(bytes).unwrap_or_else(|| mtime_rfc3339(meta))
}

pub(super) fn hex_to_phash(hex: &str) -> Option<image_hasher::ImageHash> {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(&hex[i..i+2], 16).ok())
        .collect();
    image_hasher::ImageHash::from_bytes(&bytes).ok()
}

pub(super) fn phash_to_hex(ph: &image_hasher::ImageHash) -> String {
    ph.as_bytes().iter().map(|b| format!("{:02x}", b)).collect()
}

/// Build a `CachedFile` from a `FileRecord`, supplying the two pHash variants explicitly.
pub(super) fn build_cached_file(r: &FileRecord, phash: Option<String>, fast_phash: Option<String>) -> CachedFile {
    CachedFile {
        blake3:      r.ex_hash.clone(),
        size_bytes:  r.entry.size_bytes,
        phash,
        fast_phash,
        header_hash: r.header_hash.clone(),
        width:       r.entry.width,
        height:      r.entry.height,
        modified:    r.entry.modified.clone(),
        blur_score:  r.entry.blur_score,
    }
}

/// Resolve pHash by value. Tries: records[i].ph → heic_extra → on_demand → disk.
pub(super) fn resolve_phash_owned(
    i: usize,
    records: &[FileRecord],
    heic_extra: &HashMap<usize, HeicExtra>,
    on_demand: &mut HashMap<usize, image_hasher::ImageHash>,
    fast_mode: bool,
) -> Option<image_hasher::ImageHash> {
    if let Some(ph) = &records[i].ph { return Some(ph.clone()); }
    if let Some(extra) = heic_extra.get(&i) { return Some(extra.ph.clone()); }
    if let Some(ph) = on_demand.get(&i) { return Some(ph.clone()); }
    let path = Path::new(&records[i].entry.path);
    if !is_heic(path) {
        if let Some(ph) = std::fs::read(path).ok()
            .and_then(|bytes| perceptual_hash_from_bytes(&bytes, fast_mode).ok())
        {
            on_demand.insert(i, ph.clone());
            return Some(ph);
        }
    }
    None
}

/// Read a file from disk and build a `FileRecord` without consulting the cache.
pub(super) fn make_record(path: &Path, fast_mode: bool) -> Result<FileRecord, FailedFileKind> {
    let meta = std::fs::metadata(path).map_err(|e| FailedFileKind::from_io(&e))?;
    let fs_modified = mtime_rfc3339(&meta);
    let size_bytes = meta.len();
    let path_str = path.to_string_lossy().to_string();

    let (ex_hash, _, bytes) = read_file_data(path).map_err(|e| {
        e.downcast_ref::<std::io::Error>()
            .map(FailedFileKind::from_io)
            .unwrap_or(FailedFileKind::IoError)
    })?;

    let heic = is_heic(path);
    let modified = if !heic { read_capture_date(path, &bytes, &meta) } else { fs_modified.clone() };

    let (width, height) = if !heic {
        let cursor = std::io::Cursor::new(&bytes);
        image::io::Reader::new(cursor)
            .with_guessed_format().ok()
            .and_then(|r| r.into_dimensions().ok())
            .unwrap_or((0, 0))
    } else { (0, 0) };

    let ph = if !heic {
        perceptual_hash_from_bytes(&bytes, fast_mode).ok()
    } else { None };

    let blur_score = if !heic {
        crate::hasher::laplacian_variance(&bytes)
    } else { None };

    let header_hash = Some(blake3::hash(&bytes[..bytes.len().min(4096)]).to_hex().to_string());

    Ok(FileRecord {
        entry: ImageEntry {
            path: path_str.clone(),
            size_bytes, width, height,
            modified: modified.clone(),
            blur_score,
            is_original: false,
            ..Default::default()
        },
        ex_hash: ex_hash.clone(),
        ts_tag: extract_timestamp_tag(path),
        ph: ph.clone(),
        mtime_key: fs_modified,
        header_hash,
    })
}
