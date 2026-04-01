use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering as AOrdering};

use anyhow::Result;
use rayon::prelude::*;
use unicode_normalization::UnicodeNormalization;
use walkdir::WalkDir;

use crate::cache::CachedFile;
use crate::hasher::{perceptual_hash, perceptual_hash_from_bytes, read_file_data};
use crate::heic::{batch_convert_heic, cleanup_temp};
use crate::types::{AnalyzeProgress, DuplicateGroup, ImageEntry, SimilarityKind};

static IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "bmp", "gif", "tiff", "tif", "heic", "heif",
];
static HEIC_EXTENSIONS: &[&str] = &["heic", "heif"];

fn is_image(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn is_heic(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str())
        .map(|e| HEIC_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn extract_timestamp_tag(path: &Path) -> Option<String> {
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

fn read_capture_date(path: &Path, bytes: &[u8], meta: &std::fs::Metadata) -> String {
    if let Some(ts) = extract_timestamp_tag(path) {
        if ts.len() == 15 {
            return format!("{}-{}-{}T{}:{}:{}Z",
                &ts[0..4], &ts[4..6], &ts[6..8],
                &ts[9..11], &ts[11..13], &ts[13..15]);
        }
    }
    if let Ok(exif) = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(bytes)) {
        if let Some(field) = exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY) {
            let s = field.display_value().to_string();
            if s.len() >= 19 {
                return format!("{}-{}-{}T{}:{}:{}Z",
                    &s[0..4], &s[5..7], &s[8..10],
                    &s[11..13], &s[14..16], &s[17..19]);
            }
        }
    }
    meta.modified().ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| chrono::DateTime::<chrono::Utc>::from_timestamp(d.as_secs() as i64, 0))
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}

pub fn collect_images(directories: &[PathBuf]) -> Vec<PathBuf> {
    let single_pass = || -> std::collections::HashSet<PathBuf> {
        directories.iter().flat_map(|dir| {
            WalkDir::new(dir).follow_links(false).into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .map(|e| e.into_path())
                .filter(|p| is_image(p))
        }).collect()
    };

    // Two passes: on SMB/NAS drives the first WalkDir traversal warms the
    // server-side directory cache, so a second pass consistently finds all
    // files that the cold-cache first pass may have missed.  Always take
    // the union so neither pass can silently drop files.
    let first  = single_pass();
    let second = single_pass();

    let mut all: Vec<PathBuf> = first.into_iter().chain(second)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    all.sort();
    all
}

struct FileRecord {
    entry:       ImageEntry,
    ex_hash:     String,
    ts_tag:      Option<String>,
    ph:          Option<image_hasher::ImageHash>,
    mtime_key:   String,
    header_hash: Option<String>,
}

/// Normalize a file path for use as a cache key.
/// Normalises a path to a stable cache key that survives cold/warm SMB traversal
/// differences: first apply Unicode NFC normalisation (fixes accented characters
/// like `GALERÍA` returning as NFD vs NFC across traversals), then lowercase
/// (fixes drive-letter / folder-name casing differences).
#[inline]
fn cache_key(path_str: &str) -> String {
    path_str.nfc().collect::<String>().to_lowercase()
}

/// Blake3 hash of the first 4096 bytes of a file.
/// Used as a cheap, mtime-independent cache validity check.
/// Uses take+read_to_end to guarantee reading up to 4096 bytes even on SMB/NAS,
/// where a single read() call may return fewer bytes than requested.
fn read_header_hash(path: &Path) -> Option<String> {
    use std::io::{Read, BufReader};
    let f = std::fs::File::open(path).ok()?;
    let mut buf = Vec::with_capacity(4096);
    BufReader::new(f).take(4096).read_to_end(&mut buf).ok()?;
    Some(blake3::hash(&buf).to_hex().to_string())
}

struct HeicExtra {
    ph:       image_hasher::ImageHash,
    width:    u32,
    height:   u32,
    modified: String,
}

fn hex_to_phash(hex: &str) -> Option<image_hasher::ImageHash> {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(&hex[i..i+2], 16).ok())
        .collect();
    image_hasher::ImageHash::from_bytes(&bytes).ok()
}

fn phash_to_hex(ph: &image_hasher::ImageHash) -> String {
    ph.as_bytes().iter().map(|b| format!("{:02x}", b)).collect()
}


/// Resolve pHash by value. Tries: records[i].ph → heic_extra → on_demand → disk.
fn resolve_phash_owned(
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
            .and_then(|bytes| std::panic::catch_unwind(|| perceptual_hash_from_bytes(&bytes, fast_mode)).ok().flatten())
        {
            on_demand.insert(i, ph.clone());
            return Some(ph);
        }
    }
    None
}

fn make_record(path: &Path, cache: Option<&crate::cache::Cache>, fast_mode: bool) -> Option<FileRecord> {
    let meta = std::fs::metadata(path).ok()?;

    let fs_modified = meta.modified().ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| chrono::DateTime::<chrono::Utc>::from_timestamp(d.as_secs() as i64, 0))
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());

    let size_bytes = meta.len();
    let path_str = path.to_string_lossy().to_string();

    if let Some(cache) = cache {
        if let Some(cached) = cache.get(&path_str, size_bytes, &fs_modified) {
            let ph = if fast_mode {
                cached.fast_phash.as_deref().and_then(hex_to_phash)
            } else {
                cached.phash.as_deref().and_then(hex_to_phash)
            };
            return Some(FileRecord {
                entry: ImageEntry {
                    path: path_str,
                    size_bytes: cached.size_bytes,
                    width: cached.width,
                    height: cached.height,
                    modified: cached.modified,
                    is_original: false,
                },
                ex_hash: cached.blake3,
                ts_tag: extract_timestamp_tag(path),
                ph,
                mtime_key: fs_modified,
                header_hash: cached.header_hash,
            });
        }
    }

    let (ex_hash, _, bytes) = read_file_data(path).ok()?;

    let heic = is_heic(path);
    let modified = if !heic { read_capture_date(path, &bytes, &meta) } else { fs_modified.clone() };

    let (width, height) = if !heic {
        std::panic::catch_unwind(|| {
            let cursor = std::io::Cursor::new(&bytes);
            image::io::Reader::new(cursor)
                .with_guessed_format().ok()
                .and_then(|r| r.into_dimensions().ok())
                .unwrap_or((0, 0))
        }).unwrap_or((0, 0))
    } else { (0, 0) };

    let ph = if !heic {
        std::panic::catch_unwind(|| perceptual_hash_from_bytes(&bytes, fast_mode)).ok().flatten()
    } else { None };


    let header_hash = Some(blake3::hash(&bytes[..bytes.len().min(4096)]).to_hex().to_string());

    Some(FileRecord {
        entry: ImageEntry {
            path: path_str.clone(),
            size_bytes, width, height,
            modified: modified.clone(),
            is_original: false,
        },
        ex_hash: ex_hash.clone(),
        ts_tag: extract_timestamp_tag(path),
        ph: ph.clone(),
        mtime_key: fs_modified,
        header_hash,
    })
}

pub fn find_duplicates<F1, F2>(
    directories: Vec<PathBuf>,
    prefetched_paths: Option<Vec<PathBuf>>,
    resource_dir: Option<PathBuf>,
    stop: Arc<AtomicBool>,
    phash_threshold: u32,
    cache: Option<std::sync::Arc<crate::cache::Cache>>,
    cross_date_phash: bool,
    fast_mode: bool,
    scan_cb:    F1,
    analyze_cb: F2,
) -> Result<Vec<DuplicateGroup>>
where
    F1: Fn(usize, usize) + Send + Sync,
    F2: Fn(AnalyzeProgress) + Send + Sync,
{
    // Reuse the pre-enumerated list from directory_fingerprint if available,
    // otherwise enumerate again (avoids a second SMB traversal on NAS drives).
    let paths = prefetched_paths.unwrap_or_else(|| collect_images(&directories));
    let total = paths.len();

    let mut ext_counts: HashMap<String, usize> = HashMap::new();
    for p in &paths {
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("unknown").to_lowercase();
        *ext_counts.entry(ext).or_insert(0) += 1;
    }
    let mut ext_list: Vec<_> = ext_counts.into_iter().collect();
    ext_list.sort_by(|a, b| b.1.cmp(&a.1));
    println!("[RustyMirror:RS] {} images: {}", total,
        ext_list.iter().map(|(e, n)| format!("{}: {}", e, n)).collect::<Vec<_>>().join(", "));

    // ── Phase 1: incremental scan ────────────────────────────────────────────
    let t1 = std::time::Instant::now();
    let counter = std::sync::atomic::AtomicUsize::new(0);
    let stop_phase1 = stop.clone();

    let path_strings: Vec<String> = paths.iter()
        .map(|p| cache_key(&p.to_string_lossy())).collect();

    // Diagnostic: print hex bytes of first 3 queried paths so we can compare
    // with stored paths in case of encoding discrepancies.
    for ps in path_strings.iter().take(3) {
        let hex: String = ps.bytes().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");
        println!("[RustyMirror:RS] query path hex: {:?} = {}", ps, hex);
    }

    let bulk_cache = cache.as_ref()
        .map(|c| c.get_bulk(&path_strings))
        .unwrap_or_default();

    // Diagnostic: print hex bytes of first 3 stored cache keys (keys come back
    // from get_bulk — they are the paths actually stored in SQLite).
    for (k, _) in bulk_cache.iter().take(3) {
        let hex: String = k.bytes().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");
        println!("[RustyMirror:RS] stored path hex: {:?} = {}", k, hex);
    }

    let cache_hits        = std::sync::atomic::AtomicUsize::new(0);
    let dbg_size_match    = std::sync::atomic::AtomicUsize::new(0);
    let dbg_has_hh        = std::sync::atomic::AtomicUsize::new(0);
    let dbg_hh_match      = std::sync::atomic::AtomicUsize::new(0);
    let dbg_hh_read_fail  = std::sync::atomic::AtomicUsize::new(0);
    println!("[RustyMirror:RS] cache: {} entries ({} paths to check)",
        cache.as_ref().map(|c| c.count()).unwrap_or(0), paths.len());

    // Process one file: returns the FileRecord (or None on failure/stop).
    // Extracted so the folder-sequential loop below stays readable.
    let process_one = |path: &PathBuf| -> Option<FileRecord> {
        if stop_phase1.load(AOrdering::Relaxed) { return None; }

        let path_str = path.to_string_lossy().to_string();
        let path_cache_key = cache_key(&path_str);

        let meta = std::fs::metadata(path).ok()?;
        let size  = meta.len();
        let mtime = meta.modified().ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|d| chrono::DateTime::<chrono::Utc>::from_timestamp(d.as_secs() as i64, 0))
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());

        let record = if let Some(entry) = bulk_cache.get(&path_cache_key) {
            if entry.size_bytes == size {
                dbg_size_match.fetch_add(1, AOrdering::Relaxed);
                // Validate the entry: prefer header_hash (mtime-independent),
                // fall back to mtime for old entries that don't have one yet.
                let valid = if let Some(cached_hh) = &entry.data.header_hash {
                    dbg_has_hh.fetch_add(1, AOrdering::Relaxed);
                    let actual = read_header_hash(path);
                    if actual.is_none() { dbg_hh_read_fail.fetch_add(1, AOrdering::Relaxed); }
                    let matches = actual.as_deref() == Some(cached_hh.as_str());
                    if matches { dbg_hh_match.fetch_add(1, AOrdering::Relaxed); }
                    matches
                } else {
                    entry.mtime == mtime
                };

                if valid {
                    // Check if the pHash for the requested mode is already cached.
                    let cached_ph = if fast_mode {
                        entry.data.fast_phash.as_deref().and_then(hex_to_phash)
                    } else {
                        entry.data.phash.as_deref().and_then(hex_to_phash)
                    };
                    if cached_ph.is_some() || entry.data.phash.is_none() && entry.data.fast_phash.is_none() {
                        cache_hits.fetch_add(1, AOrdering::Relaxed);
                        Some(FileRecord {
                            entry: ImageEntry {
                                path:        path_str,
                                size_bytes:  size,
                                width:       entry.data.width,
                                height:      entry.data.height,
                                modified:    entry.data.modified.clone(),
                                is_original: false,
                            },
                            ex_hash:     entry.data.blake3.clone(),
                            ts_tag:      extract_timestamp_tag(path),
                            ph:          cached_ph,
                            mtime_key:   mtime,
                            header_hash: entry.data.header_hash.clone(),
                        })
                    } else {
                        // pHash exists but only for the other mode — re-read the file.
                        std::panic::catch_unwind(|| make_record(path, None, fast_mode)).ok().flatten()
                    }
                } else {
                    // Header hash mismatch — file changed, re-read.
                    std::panic::catch_unwind(|| make_record(path, None, fast_mode)).ok().flatten()
                }
            } else {
                // Size changed — file definitely changed, re-read.
                std::panic::catch_unwind(|| make_record(path, None, fast_mode)).ok().flatten()
            }
        } else {
            // Not in DB at all — skip the redundant per-file SQLite lookup.
            std::panic::catch_unwind(|| make_record(path, None, fast_mode)).ok().flatten()
        };

        let done = counter.fetch_add(1, AOrdering::Relaxed) + 1;
        scan_cb(done, total);
        record
    };

    // ── Folder-sequential, intra-folder parallel processing ──────────────────
    // paths is already sorted, so files in the same directory are contiguous.
    // We group consecutive paths by parent dir, process each group with par_iter
    // (parallelism within a folder), then move to the next folder only after the
    // current one is fully done.  This guarantees that a cancelled scan always
    // leaves complete folders in cache, so the next scan resumes with a clean
    // fast segment rather than scattered cache hits.
    let mut results: Vec<Option<FileRecord>> = (0..paths.len()).map(|_| None).collect();

    // Build groups of (start_index, len) sharing the same parent directory.
    let mut groups: Vec<(usize, usize)> = Vec::new();
    let mut group_start = 0usize;
    while group_start < paths.len() {
        let parent = paths[group_start].parent().map(|p| p.to_path_buf());
        let mut end = group_start + 1;
        while end < paths.len() && paths[end].parent().map(|p| p.to_path_buf()) == parent {
            end += 1;
        }
        groups.push((group_start, end - group_start));
        group_start = end;
    }

    'folders: for (start, len) in groups {
        if stop_phase1.load(AOrdering::Relaxed) { break 'folders; }

        let folder_results: Vec<Option<FileRecord>> = paths[start..start + len]
            .par_iter()
            .map(|path| process_one(path))
            .collect();

        for (i, rec) in folder_results.into_iter().enumerate() {
            results[start + i] = rec;
        }
    }

    let hits = cache_hits.load(AOrdering::Relaxed);
    println!("[RustyMirror:RS] phase 1: {} hits, {} processed from disk", hits, total - hits);
    println!("[RustyMirror:RS] cache diag: size_match={} has_header_hash={} hh_match={} hh_read_fail={}",
        dbg_size_match.load(AOrdering::Relaxed),
        dbg_has_hh.load(AOrdering::Relaxed),
        dbg_hh_match.load(AOrdering::Relaxed),
        dbg_hh_read_fail.load(AOrdering::Relaxed));

    let records: Vec<FileRecord> = results.into_iter().flatten().collect();
    println!("[RustyMirror:RS] phase 1 done: {} records in {:.1}s", records.len(), t1.elapsed().as_secs_f32());

    // Always persist to cache — even on cancellation, so partial results aren't lost.
    if let Some(ref c) = cache {
        let to_cache: Vec<(String, String, CachedFile)> = records.iter()
            .map(|r| {
                let ph_hex = r.ph.as_ref().map(phash_to_hex);
                // Preserve the pHash for the other mode from the bulk cache, if present.
                let existing = bulk_cache.get(&cache_key(&r.entry.path));
                let (phash, fast_phash) = if fast_mode {
                    (existing.and_then(|e| e.data.phash.clone()), ph_hex)
                } else {
                    (ph_hex, existing.and_then(|e| e.data.fast_phash.clone()))
                };
                (cache_key(&r.entry.path), r.mtime_key.clone(), CachedFile {
                    blake3:      r.ex_hash.clone(),
                    size_bytes:  r.entry.size_bytes,
                    phash,
                    fast_phash,
                    header_hash: r.header_hash.clone(),
                    width:       r.entry.width,
                    height:      r.entry.height,
                    modified:    r.entry.modified.clone(),
                })
            })
            .collect();
        // Diagnostic: print hex bytes of first 3 paths being stored.
        for (k, _, _) in to_cache.iter().take(3) {
            let hex: String = k.bytes().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");
            println!("[RustyMirror:RS] write path hex: {:?} = {}", k, hex);
        }
        if let Err(e) = c.put_batch(&to_cache) {
            println!("[RustyMirror:RS] cache write error: {}", e);
        } else {
            println!("[RustyMirror:RS] cache: wrote {} entries", to_cache.len());
        }
    }

    if stop.load(AOrdering::Relaxed) {
        println!("[RustyMirror:RS] scan stopped by user — partial cache saved ({} records)", records.len());
        return Ok(vec![]);
    }

    let mut grouped = vec![false; records.len()];
    let mut groups: Vec<DuplicateGroup> = Vec::new();

    // ── Phase 2: exact Blake3 hash ────────────────────────────────────────────
    analyze_cb(AnalyzeProgress { analyzed: 0, total: records.len(),
        phase: "Grouping exact duplicates…".into() });
    let mut exact_map: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, r) in records.iter().enumerate() {
        exact_map.entry(&r.ex_hash).or_default().push(i);
    }
    for (_, indices) in &exact_map {
        if indices.len() < 2 { continue; }
        let mut entries: Vec<ImageEntry> = indices.iter().map(|&i| records[i].entry.clone()).collect();
        mark_original(&mut entries); sort_by_date(&mut entries);
        indices.iter().for_each(|&i| grouped[i] = true);
        groups.push(DuplicateGroup { kind: SimilarityKind::Exact, entries, similarity: Some(100) });
    }
    println!("[RustyMirror:RS] phase 2: {} exact groups", groups.len());

    // ── Phase 3: perceptual hash (user threshold) ─────────────────────────────
    if stop.load(AOrdering::Relaxed) {
        println!("[RustyMirror:RS] scan stopped by user before phase 3");
        return Ok(groups);
    }
    let ungrouped_ph: Vec<usize> = (0..records.len()).filter(|&i| !grouped[i]).collect();
    println!("[RustyMirror:RS] phase 3 (pHash): {} files to compare", ungrouped_ph.len());

    let t3 = std::time::Instant::now();

    let all_heic_indices: Vec<usize> = (0..records.len())
        .filter(|&i| is_heic(Path::new(&records[i].entry.path))).collect();
    let ungrouped_heic_indices: Vec<usize> = ungrouped_ph.iter().cloned()
        .filter(|&i| is_heic(Path::new(&records[i].entry.path))).collect();

    let heic_need_convert: Vec<usize> = ungrouped_heic_indices.iter().cloned()
        .filter(|&i| records[i].ph.is_none()).collect();
    let heic_convert_paths: Vec<PathBuf> = heic_need_convert.iter()
        .map(|&i| PathBuf::from(&records[i].entry.path)).collect();
    let heic_count = heic_convert_paths.len();

    println!("[RustyMirror:RS] phase 3: {} HEIC ({} need conversion, {} from cache)",
        ungrouped_heic_indices.len(), heic_count,
        ungrouped_heic_indices.len() - heic_count);

    analyze_cb(AnalyzeProgress { analyzed: 0, total: heic_count.max(1),
        phase: format!("Converting {} HEIC files…", heic_count) });

    let conversions: HashMap<PathBuf, (PathBuf, u32, u32)> = if heic_count > 0 {
        batch_convert_heic(&heic_convert_paths, resource_dir.as_deref(), |done, total| {
            analyze_cb(AnalyzeProgress { analyzed: done, total: total.max(1),
                phase: format!("Converting HEIC files ({}/{})…", done, total) });
        }).into_iter().map(|(orig, tmp, w, h)| (orig, (tmp, w, h))).collect()
    } else { HashMap::new() };

    println!("[RustyMirror:RS] phase 3a: {}/{} HEIC converted in {:.1}s",
        conversions.len(), heic_count, t3.elapsed().as_secs_f32());

    let t3b = std::time::Instant::now();

    let stop_ph = stop.clone();
    let mut heic_extra: HashMap<usize, HeicExtra> = heic_need_convert
        .par_iter()
        .filter_map(|&i| {
            if stop_ph.load(AOrdering::Relaxed) { return None; }
            let orig = PathBuf::from(&records[i].entry.path);
            let (tmp, w, h) = conversions.get(&orig)?;
            let tmp_bytes = std::fs::read(tmp).ok()?;
            let ph = std::panic::catch_unwind(|| perceptual_hash(tmp, fast_mode)).ok().flatten()?;
            let modified = {
                if let Ok(exif) = exif::Reader::new()
                    .read_from_container(&mut std::io::Cursor::new(&tmp_bytes))
                {
                    if let Some(field) = exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY) {
                        let s = field.display_value().to_string();
                        if s.len() >= 19 {
                            format!("{}-{}-{}T{}:{}:{}Z", &s[0..4], &s[5..7], &s[8..10], &s[11..13], &s[14..16], &s[17..19])
                        } else { records[i].entry.modified.clone() }
                    } else { records[i].entry.modified.clone() }
                } else { records[i].entry.modified.clone() }
            };
            Some((i, HeicExtra { ph, width: *w, height: *h, modified }))
        })
        .collect();

    for &i in &ungrouped_heic_indices {
        if heic_extra.contains_key(&i) { continue; }
        if let Some(ph) = records[i].ph.clone() {
            heic_extra.insert(i, HeicExtra {
                ph,
                width:    records[i].entry.width,
                height:   records[i].entry.height,
                modified: records[i].entry.modified.clone(),
            });
        }
    }

    for (tmp, _, _) in conversions.values() { cleanup_temp(tmp); }
    println!("[RustyMirror:RS] phase 3b: {} HEIC pHashes in {:.1}s", heic_extra.len(), t3b.elapsed().as_secs_f32());

    if let Some(ref c) = cache {
        let heic_updates: Vec<(String, String, CachedFile)> = heic_extra.iter()
            .map(|(&i, extra)| {
                let r = &records[i];
                (cache_key(&r.entry.path), r.mtime_key.clone(), CachedFile {
                    blake3:      r.ex_hash.clone(),
                    size_bytes:  r.entry.size_bytes,
                    phash:       Some(phash_to_hex(&extra.ph)),
                    fast_phash:  None,
                    header_hash: r.header_hash.clone(),
                    width:       extra.width,
                    height:      extra.height,
                    modified:    extra.modified.clone(),
                })
            }).collect();
        let _ = c.put_batch(&heic_updates);
    }

    let grouped_heic_indices: Vec<usize> = all_heic_indices.iter().cloned()
        .filter(|&i| grouped[i]).collect();
    let grouped_heic_dims: HashMap<usize, (u32, u32)> = grouped_heic_indices
        .par_iter()
        .filter_map(|&i| {
            let path = Path::new(&records[i].entry.path);
            let (w, h) = crate::heic::heic_dimensions(path, resource_dir.as_deref());
            if w > 0 { Some((i, (w, h))) } else { None }
        })
        .collect();

    let entry_corrected = |i: usize| -> ImageEntry {
        let mut e = records[i].entry.clone();
        if let Some(extra) = heic_extra.get(&i) {
            if extra.width > 0  { e.width  = extra.width;  }
            if extra.height > 0 { e.height = extra.height; }
            e.modified = extra.modified.clone();
        } else if let Some(&(w, h)) = grouped_heic_dims.get(&i) {
            e.width  = w;
            e.height = h;
        }
        e
    };

    for group in &mut groups {
        for entry in &mut group.entries {
            let path = PathBuf::from(&entry.path);
            if is_heic(&path) {
                if let Some(i) = records.iter().position(|r| r.entry.path == entry.path) {
                    if let Some(extra) = heic_extra.get(&i) {
                        if extra.width > 0  { entry.width  = extra.width;  }
                        if extra.height > 0 { entry.height = extra.height; }
                        entry.modified = extra.modified.clone();
                    } else if let Some(&(w, h)) = grouped_heic_dims.get(&i) {
                        entry.width  = w;
                        entry.height = h;
                    }
                }
            }
        }
    }

    // 3c — pHash comparison with user threshold
    analyze_cb(AnalyzeProgress { analyzed: 0, total: ungrouped_ph.len(),
        phase: "Comparing similar images…".into() });

    let ph_pairs: Vec<(usize, &image_hasher::ImageHash)> = ungrouped_ph.iter()
        .filter_map(|&i| {
            let ph = records[i].ph.as_ref()
                .or_else(|| heic_extra.get(&i).map(|e| &e.ph))?;
            Some((i, ph))
        })
        .collect();

    println!("[RustyMirror:RS] phase 3c: comparing {} pHashes", ph_pairs.len());
    let t3c = std::time::Instant::now();
    let n = ph_pairs.len();
    let mut ph_grouped = vec![false; n];

    // Complete-linkage greedy clustering:
    // B is added to a cluster only if dist(B, X) ≤ threshold for EVERY existing
    // member X. This guarantees that the worst-case pairwise distance inside any
    // cluster never exceeds the threshold, so the displayed similarity is always
    // ≥ the value the user selected in the slider.
    for a in 0..n {
        if ph_grouped[a] { continue; }
        let mut cluster: Vec<usize> = vec![a];

        'next_b: for b in (a + 1)..n {
            if ph_grouped[b] { continue; }
            for &x in &cluster {
                if ph_pairs[x].1.dist(ph_pairs[b].1) > phash_threshold {
                    continue 'next_b; // too far from some cluster member — skip
                }
            }
            cluster.push(b);
            ph_grouped[b] = true;
        }

        if cluster.len() < 2 { continue; }
        ph_grouped[a] = true;

        let mut max_dist = 0u32;
        for x in 0..cluster.len() {
            for y in (x + 1)..cluster.len() {
                let d = ph_pairs[cluster[x]].1.dist(ph_pairs[cluster[y]].1);
                if d > max_dist { max_dist = d; }
            }
        }
        // max_dist ≤ phash_threshold is guaranteed by construction
        let (kind, similarity) = if max_dist == 0 {
            (SimilarityKind::Exact, Some(100u8))
        } else {
            let pct = (((64 - max_dist) as f32 / 64.0) * 100.0).round() as u8;
            (SimilarityKind::Similar, Some(pct))
        };
        let mut entries: Vec<ImageEntry> = cluster.iter()
            .map(|&pos| entry_corrected(ph_pairs[pos].0)).collect();
        mark_original(&mut entries); sort_by_date(&mut entries);
        for &pos in &cluster { grouped[ph_pairs[pos].0] = true; }
        groups.push(DuplicateGroup { kind, entries, similarity });

        if a % 100 == 0 || a == n.saturating_sub(1) {
            analyze_cb(AnalyzeProgress { analyzed: a + 1, total: n,
                phase: "Comparing similar images…".into() });
        }
    }
    println!("[RustyMirror:RS] phase 3c done in {:.1}s", t3c.elapsed().as_secs_f32());

    // ── Phase 4: timestamp tag (fallback) ─────────────────────────────────────
    // Accumulates pHashes for sameDate members with NULL phash in older cache entries.
    let mut on_demand_phashes: HashMap<usize, image_hasher::ImageHash> = HashMap::new();

    analyze_cb(AnalyzeProgress { analyzed: 0, total: records.len(),
        phase: "Matching by filename timestamp…".into() });
    let mut ts_map: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, r) in records.iter().enumerate() {
        if grouped[i] { continue; }
        if let Some(tag) = &r.ts_tag { ts_map.entry(tag.as_str()).or_default().push(i); }
    }

    // samedate_group_indices tracks which record indices belong to each sameDate
    // group (by group index in `groups`), needed for the cross-group phase 5.
    let mut samedate_group_indices: Vec<Vec<usize>> = Vec::new();

    let before = groups.len();
    for (_, indices) in &ts_map {
        if indices.len() < 2 { continue; }

        // Compute intra-group similarity using minimum threshold (Hamming ≤ 16, ~75%)
        let min_hamming: u32 = 16;
        let member_phashes: Vec<Option<image_hasher::ImageHash>> = indices.iter()
            .map(|&i| resolve_phash_owned(i, &records, &heic_extra, &mut on_demand_phashes, fast_mode))
            .collect();

        let similarity = if member_phashes.iter().all(|ph| ph.is_some()) {
            let phashes: Vec<&image_hasher::ImageHash> = member_phashes.iter()
                .map(|ph| ph.as_ref().unwrap())
                .collect();
            let mut max_dist = 0u32;
            for x in 0..phashes.len() {
                for y in (x+1)..phashes.len() {
                    let d = phashes[x].dist(phashes[y]);
                    if d > max_dist { max_dist = d; }
                }
            }
            if max_dist <= min_hamming {
                let pct = (((64 - max_dist) as f32 / 64.0) * 100.0).round() as u8;
                Some(pct)
            } else { None }
        } else { None };

        let mut entries: Vec<ImageEntry> = indices.iter().map(|&i| records[i].entry.clone()).collect();
        mark_original(&mut entries); sort_by_date(&mut entries);
        indices.iter().for_each(|&i| grouped[i] = true);
        samedate_group_indices.push(indices.to_vec());
        groups.push(DuplicateGroup { kind: SimilarityKind::SameDate, entries, similarity });
    }
    println!("[RustyMirror:RS] phase 4: {} timestamp groups", groups.len() - before);

    // Persist on-demand pHashes to cache
    if let Some(ref c) = cache {
        let updates: Vec<(String, String, CachedFile)> = on_demand_phashes.iter()
            .map(|(&i, ph)| {
                let r = &records[i];
                (cache_key(&r.entry.path), r.mtime_key.clone(), CachedFile {
                    blake3:      r.ex_hash.clone(),
                    size_bytes:  r.entry.size_bytes,
                    phash:       Some(phash_to_hex(ph)),
                    fast_phash:  None,
                    header_hash: r.header_hash.clone(),
                    width:       r.entry.width,
                    height:      r.entry.height,
                    modified:    r.entry.modified.clone(),
                })
            }).collect();
        if !updates.is_empty() {
            println!("[RustyMirror:RS] phase 4: writing {} on-demand pHashes to cache", updates.len());
            let _ = c.put_batch(&updates);
        }
    }

    // ── Phase 5: cross-group pHash between all sameDate members ───────────────
    // Compares all members across all sameDate groups using the minimum threshold
    // (Hamming ≤ 16, ~75%). Cross-group clusters are emitted as SameDate groups
    // with a similarity percentage. Members absorbed into a cross-group cluster
    // are removed from their original sameDate group; groups that become empty
    // or have only one member left are discarded.
    if cross_date_phash && samedate_group_indices.len() >= 2 {
        println!("[RustyMirror:RS] phase 5: cross-group pHash across {} sameDate groups", samedate_group_indices.len());

        let min_hamming: u32 = 16;

        // flat[i] = (record_idx, samedate_group_idx)
        let mut flat: Vec<(usize, usize)> = Vec::new();
        for (g, members) in samedate_group_indices.iter().enumerate() {
            for &r in members {
                flat.push((r, g));
            }
        }

        let ph_flat: Vec<Option<image_hasher::ImageHash>> = flat.iter()
            .map(|&(ri, _)| resolve_phash_owned(ri, &records, &heic_extra, &mut on_demand_phashes, fast_mode))
            .collect();

        let m = flat.len();
        let mut sd_grouped = vec![false; m];

        // For each sameDate group: which flat indices were absorbed by a cross-group cluster
        let mut absorbed_by_group: Vec<std::collections::HashSet<usize>> =
            vec![std::collections::HashSet::new(); samedate_group_indices.len()];

        let mut new_groups: Vec<DuplicateGroup> = Vec::new();

        for a in 0..m {
            if sd_grouped[a] { continue; }
            let ph_a = match &ph_flat[a] { Some(ph) => ph, None => continue };

            let mut cluster = vec![a];
            for b in (a + 1)..m {
                if sd_grouped[b] { continue; }
                if flat[a].1 == flat[b].1 { continue; } // skip same-group pairs
                if let Some(ph_b) = &ph_flat[b] {
                    if ph_a.dist(ph_b) <= min_hamming {
                        cluster.push(b);
                        sd_grouped[b] = true;
                    }
                }
            }

            if cluster.len() < 2 { continue; }

            // Must span at least two different sameDate groups
            let groups_in_cluster: std::collections::HashSet<usize> =
                cluster.iter().map(|&idx| flat[idx].1).collect();
            if groups_in_cluster.len() < 2 { continue; }

            sd_grouped[a] = true;

            // Worst-case distance across the cluster
            let mut max_dist = 0u32;
            for x in 0..cluster.len() {
                for y in (x+1)..cluster.len() {
                    if let (Some(px), Some(py)) = (&ph_flat[cluster[x]], &ph_flat[cluster[y]]) {
                        let d = px.dist(py);
                        if d > max_dist { max_dist = d; }
                    }
                }
            }

            let similarity = if max_dist == 0 {
                Some(100u8)
            } else {
                Some((((64 - max_dist) as f32 / 64.0) * 100.0).round() as u8)
            };

            // Mark absorbed members per source group so we can prune them
            for &idx in &cluster {
                absorbed_by_group[flat[idx].1].insert(idx);
            }

            let mut entries: Vec<ImageEntry> = cluster.iter()
                .map(|&idx| entry_corrected(flat[idx].0))
                .collect();
            mark_original(&mut entries); sort_by_date(&mut entries);

            // Cross-group clusters are labelled SameDate with a similarity score
            // so the user can see they were linked by date AND visual similarity.
            new_groups.push(DuplicateGroup {
                kind: SimilarityKind::SameDate,
                entries,
                similarity,
            });
        }

        if !new_groups.is_empty() {
            // Rebuild sameDate groups: remove absorbed members, drop groups with < 2 left.
            // groups[before + g] corresponds to samedate_group_indices[g].
            let samedate_start = before;
            let num_samedate = samedate_group_indices.len();

            // Build set of paths absorbed from each sameDate group
            let absorbed_paths: Vec<std::collections::HashSet<String>> = (0..num_samedate)
                .map(|g| {
                    absorbed_by_group[g].iter()
                        .map(|&idx| records[flat[idx].0].entry.path.clone())
                        .collect()
                })
                .collect();

            // Patch existing sameDate groups in-place
            let mut groups_to_remove: std::collections::HashSet<usize> = std::collections::HashSet::new();
            for g in 0..num_samedate {
                if absorbed_paths[g].is_empty() { continue; }
                let group_pos = samedate_start + g;
                if group_pos >= groups.len() { continue; }
                // Remove absorbed entries
                groups[group_pos].entries.retain(|e| !absorbed_paths[g].contains(&e.path));
                // If fewer than 2 members remain, mark for removal
                if groups[group_pos].entries.len() < 2 {
                    groups_to_remove.insert(group_pos);
                } else {
                    // Re-mark original among remaining entries
                    mark_original(&mut groups[group_pos].entries);
                }
            }

            let fully_absorbed = groups_to_remove.len();
            if !groups_to_remove.is_empty() {
                let mut i = 0usize;
                groups.retain(|_| { let keep = !groups_to_remove.contains(&i); i += 1; keep });
            }

            println!("[RustyMirror:RS] phase 5: {} cross-group clusters, {} sameDate groups removed, {} partially pruned",
                new_groups.len(),
                fully_absorbed,
                absorbed_paths.iter().filter(|s| !s.is_empty()).count().saturating_sub(fully_absorbed));

            groups.extend(new_groups);
        } else {
            println!("[RustyMirror:RS] phase 5: no cross-group clusters found");
        }
    }

    groups.sort_by(|a, b| {
        a.entries.first().map(|e| e.modified.as_str())
            .cmp(&b.entries.first().map(|e| e.modified.as_str()))
    });

    println!("[RustyMirror:RS] complete: {} groups", groups.len());
    Ok(groups)
}

fn is_non_original_filename(path: &str) -> bool {
    let stem = std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    let lower = stem.to_lowercase();

    // Rule 1: name explicitly marks it as a copy
    if lower.contains("copy") || lower.contains("copia") {
        return true;
    }

    // Rule 2: name does not follow canonical pattern IMG_YYYYMMDD_HHMMSS_XXXX
    // where XXXX is exactly 4 alphanumeric characters
    let canonical = regex_is_canonical(stem);
    !canonical
}

fn regex_is_canonical(stem: &str) -> bool {
    // Pattern: IMG_ + 8 digits + _ + 6 digits + _ + 4 alphanumeric chars
    // Example: IMG_20210828_132922_A23C
    if !stem.starts_with("IMG_") { return false; }
    let rest = &stem[4..]; // skip "IMG_"
    // Expect: 8 digits
    if rest.len() < 8 { return false; }
    if !rest[..8].chars().all(|c| c.is_ascii_digit()) { return false; }
    let rest = &rest[8..]; // skip date
    // Expect: _ + 6 digits
    if !rest.starts_with('_') { return false; }
    let rest = &rest[1..];
    if rest.len() < 6 { return false; }
    if !rest[..6].chars().all(|c| c.is_ascii_digit()) { return false; }
    let rest = &rest[6..]; // skip time
    // Expect: _ + exactly 4 alphanumeric chars, nothing more
    if !rest.starts_with('_') { return false; }
    let rest = &rest[1..];
    rest.len() == 4 && rest.chars().all(|c| c.is_ascii_alphanumeric())
}

fn mark_original(entries: &mut Vec<ImageEntry>) {
    let has_canonical = entries.iter().any(|e| !is_non_original_filename(&e.path));

    let candidates: Vec<usize> = entries.iter().enumerate()
        .filter(|(_, e)| !has_canonical || !is_non_original_filename(&e.path))
        .map(|(i, _)| i)
        .collect();

    if let Some(&best) = candidates.iter()
        .max_by_key(|&&i| (entries[i].size_bytes, entries[i].width as u64 * entries[i].height as u64))
    {
        entries[best].is_original = true;
    }
}

fn sort_by_date(entries: &mut Vec<ImageEntry>) {
    entries.sort_by(|a, b| a.modified.cmp(&b.modified));
}
