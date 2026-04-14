mod bktree;
mod grouping;
mod record;
mod walk;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering as AOrdering};

use anyhow::Result;
use rayon::prelude::*;

use crate::cache::CachedFile;
use crate::heic::{batch_convert_heic, cleanup_temp};
use crate::types::{AnalyzeProgress, DuplicateGroup, FailedFile, ImageEntry, RetentionRule, SimilarityKind};

use self::bktree::BkTree;
use self::grouping::{mark_original, sort_by_date};
use self::record::{
    FileRecord, HeicExtra,
    build_cached_file, cache_key, hex_to_phash, make_record,
    mtime_rfc3339, parse_exif_date, phash_to_hex, read_header_hash,
    resolve_phash_owned,
};
use self::walk::is_heic;

pub use self::walk::collect_images;

// Upper bound for the O(n²) distance matrix used in phase 5 cross-date comparison.
// Above this size the matrix is skipped to avoid large allocations.
const MATRIX_LIMIT: usize = 4_000;

pub fn find_duplicates<F1, F2>(
    directories: Vec<PathBuf>,
    prefetched_paths: Option<Vec<PathBuf>>,
    resource_dir: Option<PathBuf>,
    stop: Arc<AtomicBool>,
    phash_threshold: u32,
    cache: Option<std::sync::Arc<crate::cache::Cache>>,
    cross_date_phash: bool,
    fast_mode: bool,
    retention_rule: RetentionRule,
    scan_cb:    F1,
    analyze_cb: F2,
) -> Result<(Vec<DuplicateGroup>, Vec<FailedFile>)>
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
    tracing::debug!("{} images: {}", total,
        ext_list.iter().map(|(e, n)| format!("{}: {}", e, n)).collect::<Vec<_>>().join(", "));

    // ── Phase 1: incremental scan ────────────────────────────────────────────
    let _p1 = tracing::info_span!("phase1_hash", total).entered();
    let t1 = std::time::Instant::now();
    let counter = std::sync::atomic::AtomicUsize::new(0);
    let stop_phase1 = stop.clone();

    let path_strings: Vec<String> = paths.iter()
        .map(|p| cache_key(&p.to_string_lossy())).collect();

    let bulk_cache = cache.as_ref()
        .map(|c| c.get_bulk(&path_strings))
        .unwrap_or_default();

    let cache_hits        = std::sync::atomic::AtomicUsize::new(0);
    let dbg_size_match    = std::sync::atomic::AtomicUsize::new(0);
    let dbg_has_hh        = std::sync::atomic::AtomicUsize::new(0);
    let dbg_hh_match      = std::sync::atomic::AtomicUsize::new(0);
    let dbg_hh_read_fail  = std::sync::atomic::AtomicUsize::new(0);
    tracing::debug!("cache: {} entries ({} paths to check)",
        cache.as_ref().map(|c| c.count()).unwrap_or(0), paths.len());

    // Process one file: returns the FileRecord (or None on failure/stop).
    // Extracted so the folder-sequential loop below stays readable.
    let process_one = |path: &PathBuf| -> Option<FileRecord> {
        if stop_phase1.load(AOrdering::Relaxed) { return None; }

        let path_str = path.to_string_lossy().to_string();
        let path_cache_key = cache_key(&path_str);

        let meta = std::fs::metadata(path).ok()?;
        let size  = meta.len();
        let mtime = mtime_rfc3339(&meta);

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
                                blur_score:  entry.data.blur_score,
                                is_original: false,
                                ..Default::default()
                            },
                            ex_hash:     entry.data.blake3.clone(),
                            ts_tag:      record::extract_timestamp_tag(path),
                            ph:          cached_ph,
                            mtime_key:   mtime,
                            header_hash: entry.data.header_hash.clone(),
                        })
                    } else {
                        // pHash exists but only for the other mode — re-read the file.
                        make_record(path, fast_mode)
                    }
                } else {
                    // Header hash mismatch — file changed, re-read.
                    make_record(path, fast_mode)
                }
            } else {
                // Size changed — file definitely changed, re-read.
                make_record(path, fast_mode)
            }
        } else {
            // Not in DB at all — skip the redundant per-file SQLite lookup.
            make_record(path, fast_mode)
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
    let mut folder_groups: Vec<(usize, usize)> = Vec::new();
    let mut group_start = 0usize;
    while group_start < paths.len() {
        let parent = paths[group_start].parent().map(|p| p.to_path_buf());
        let mut end = group_start + 1;
        while end < paths.len() && paths[end].parent().map(|p| p.to_path_buf()) == parent {
            end += 1;
        }
        folder_groups.push((group_start, end - group_start));
        group_start = end;
    }

    'folders: for (start, len) in folder_groups {
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
    tracing::debug!(
        cache.hits = hits,
        cache.misses = total - hits,
        cache.hit_pct = hits * 100 / total.max(1),
        cache.size_match = dbg_size_match.load(AOrdering::Relaxed),
        cache.has_header_hash = dbg_has_hh.load(AOrdering::Relaxed),
        cache.hh_match = dbg_hh_match.load(AOrdering::Relaxed),
        cache.hh_read_fail = dbg_hh_read_fail.load(AOrdering::Relaxed),
        "phase 1 cache stats"
    );

    // Collect failed files: paths where result is None and scan was not stopped.
    let failed_files: Vec<FailedFile> = if stop.load(AOrdering::Relaxed) {
        vec![]
    } else {
        results.iter().zip(paths.iter())
            .filter(|(r, _)| r.is_none())
            .map(|(_, p)| FailedFile {
                path: p.to_string_lossy().to_string(),
                reason: "Failed to read or decode image".to_string(),
            })
            .collect()
    };
    if !failed_files.is_empty() {
        tracing::debug!("phase 1: {} files failed to read", failed_files.len());
    }
    let records: Vec<FileRecord> = results.into_iter().flatten().collect();
    tracing::info!(
        records = records.len(),
        failed = failed_files.len(),
        elapsed_ms = t1.elapsed().as_millis(),
        "phase 1 done"
    );
    drop(_p1);

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
                (cache_key(&r.entry.path), r.mtime_key.clone(), build_cached_file(r, phash, fast_phash))
            })
            .collect();
        if let Err(e) = c.put_batch(&to_cache) {
            tracing::debug!("cache write error: {}", e);
        } else {
            tracing::debug!("cache: wrote {} entries", to_cache.len());
        }
    }

    if stop.load(AOrdering::Relaxed) {
        tracing::debug!("scan stopped by user — partial cache saved ({} records)", records.len());
        return Ok((vec![], vec![]));
    }

    let mut grouped = vec![false; records.len()];
    let mut groups: Vec<DuplicateGroup> = Vec::new();

    // ── Phase 2: exact Blake3 hash ────────────────────────────────────────────
    let _p2 = tracing::info_span!("phase2_exact", records = records.len()).entered();
    analyze_cb(AnalyzeProgress { analyzed: 0, total: records.len(),
        phase: "Grouping exact duplicates…".into() });
    let mut exact_map: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, r) in records.iter().enumerate() {
        exact_map.entry(&r.ex_hash).or_default().push(i);
    }
    for (_, indices) in &exact_map {
        if indices.len() < 2 { continue; }
        let mut entries: Vec<ImageEntry> = indices.iter().map(|&i| records[i].entry.clone()).collect();
        mark_original(&mut entries, &retention_rule); sort_by_date(&mut entries);
        indices.iter().for_each(|&i| grouped[i] = true);
        groups.push(DuplicateGroup { kind: SimilarityKind::Exact, entries, similarity: Some(100) });
    }
    tracing::info!(exact_groups = groups.len(), "phase 2 done");
    drop(_p2);

    // ── Phase 3: perceptual hash (user threshold) ─────────────────────────────
    if stop.load(AOrdering::Relaxed) {
        tracing::debug!("scan stopped by user before phase 3");
        return Ok((groups, failed_files));
    }
    let ungrouped_ph: Vec<usize> = (0..records.len()).filter(|&i| !grouped[i]).collect();
    let _p3 = tracing::info_span!("phase3_phash", candidates = ungrouped_ph.len()).entered();
    tracing::debug!("phase 3 (pHash): {} files to compare", ungrouped_ph.len());

    let t3 = std::time::Instant::now();

    let all_heic_indices: Vec<usize> = (0..records.len())
        .filter(|&i| is_heic(Path::new(&records[i].entry.path))).collect();
    let ungrouped_heic_indices: Vec<usize> = ungrouped_ph.iter().cloned()
        .filter(|&i| is_heic(Path::new(&records[i].entry.path))).collect();

    let heic_need_convert: Vec<usize> = ungrouped_heic_indices.iter().cloned()
        .filter(|&i| records[i].ph.is_none()).collect();
    // Grouped HEICs (exact duplicates) whose dimensions are 0 — include them
    // in the same batch conversion to avoid spawning magick identify per file later.
    let heic_need_dims_only: Vec<usize> = all_heic_indices.iter().cloned()
        .filter(|&i| grouped[i] && records[i].entry.width == 0).collect();
    let heic_convert_paths: Vec<PathBuf> = heic_need_convert.iter().chain(heic_need_dims_only.iter())
        .map(|&i| PathBuf::from(&records[i].entry.path)).collect();
    let heic_count = heic_need_convert.len();
    let heic_total_convert = heic_convert_paths.len();

    tracing::debug!("phase 3: {} HEIC ({} need pHash, {} from cache, {} grouped need dims)",
        ungrouped_heic_indices.len(), heic_count,
        ungrouped_heic_indices.len() - heic_count,
        heic_need_dims_only.len());

    analyze_cb(AnalyzeProgress { analyzed: 0, total: heic_total_convert.max(1),
        phase: format!("Converting {} HEIC files…", heic_total_convert) });

    let conversions: HashMap<PathBuf, (PathBuf, u32, u32)> = if heic_total_convert > 0 {
        // max_dim=512: temp JPEGs are ~50× smaller than full-res, cutting I/O dramatically.
        // pHash quality is identical — it only needs a small image.
        batch_convert_heic(&heic_convert_paths, resource_dir.as_deref(), Some(512), |done, total| {
            analyze_cb(AnalyzeProgress { analyzed: done, total: total.max(1),
                phase: format!("Converting HEIC files ({}/{})…", done, total) });
        }).into_iter().map(|(orig, tmp, w, h)| (orig, (tmp, w, h))).collect()
    } else { HashMap::new() };

    tracing::debug!("phase 3a: {}/{} HEIC converted in {:.1}s",
        conversions.len(), heic_total_convert, t3.elapsed().as_secs_f32());

    let t3b = std::time::Instant::now();

    let stop_ph = stop.clone();
    let phase3b_total = heic_need_convert.len();
    let phase3b_counter = std::sync::atomic::AtomicUsize::new(0);
    analyze_cb(AnalyzeProgress { analyzed: 0, total: phase3b_total.max(1),
        phase: format!("Hashing {} HEIC images…", phase3b_total) });

    let mut heic_extra: HashMap<usize, HeicExtra> = heic_need_convert
        .par_iter()
        .filter_map(|&i| {
            if stop_ph.load(AOrdering::Relaxed) { return None; }
            let orig = PathBuf::from(&records[i].entry.path);
            let (tmp, w, h) = conversions.get(&orig)?;
            let tmp_bytes = std::fs::read(tmp).ok()?;
            // Reuse already-read bytes — avoids a second disk read of the temp JPEG.
            let ph = crate::hasher::perceptual_hash_from_bytes(&tmp_bytes, fast_mode).ok()?;
            let modified = parse_exif_date(&tmp_bytes)
                .unwrap_or_else(|| records[i].entry.modified.clone());
            let done = phase3b_counter.fetch_add(1, AOrdering::Relaxed) + 1;
            analyze_cb(AnalyzeProgress { analyzed: done, total: phase3b_total.max(1),
                phase: "Hashing HEIC images…".into() });
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
    tracing::debug!("phase 3b: {} HEIC pHashes in {:.1}s", heic_extra.len(), t3b.elapsed().as_secs_f32());

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
                    blur_score:  None,
                })
            })
            // Also cache dimensions for grouped HEICs we just converted, so future
            // scans find them in cache (w > 0) and skip the conversion entirely.
            .chain(heic_need_dims_only.iter().filter_map(|&i| {
                let orig = PathBuf::from(&records[i].entry.path);
                let (_, w, h) = conversions.get(&orig)?;
                if *w == 0 { return None; }
                let r = &records[i];
                let key = cache_key(&r.entry.path);
                let existing = bulk_cache.get(&key);
                Some((key, r.mtime_key.clone(), CachedFile {
                    blake3:      r.ex_hash.clone(),
                    size_bytes:  r.entry.size_bytes,
                    phash:       existing.and_then(|e| e.data.phash.clone()),
                    fast_phash:  existing.and_then(|e| e.data.fast_phash.clone()),
                    header_hash: r.header_hash.clone(),
                    width:       *w,
                    height:      *h,
                    modified:    r.entry.modified.clone(),
                    blur_score:  None,
                }))
            }))
            .collect();
        let _ = c.put_batch(&heic_updates);
    }

    let grouped_heic_indices: Vec<usize> = all_heic_indices.iter().cloned()
        .filter(|&i| grouped[i]).collect();
    // Build dimensions map for grouped HEICs from conversion results — no subprocess.
    // Falls back to cached dimensions (width > 0 from a prior scan) if not converted.
    let grouped_heic_dims: HashMap<usize, (u32, u32)> = grouped_heic_indices
        .iter()
        .filter_map(|&i| {
            let orig = PathBuf::from(&records[i].entry.path);
            if let Some((_, w, h)) = conversions.get(&orig) {
                if *w > 0 { return Some((i, (*w, *h))); }
            }
            if records[i].entry.width > 0 {
                return Some((i, (records[i].entry.width, records[i].entry.height)));
            }
            None
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

    tracing::debug!("phase 3c: comparing {} pHashes", ph_pairs.len());
    let t3c = std::time::Instant::now();
    let n = ph_pairs.len();
    let mut ph_grouped = vec![false; n];

    // Build a BK-tree for O(n log n) candidate lookup instead of O(n²) scan.
    // For each ungrouped image A, the tree returns only the images within
    // `phash_threshold` Hamming distance — skipping the rest entirely.
    // Since A is always in its own cluster, complete-linkage requires every
    // member to be within threshold of A, so querying on A's hash yields the
    // complete candidate set without missing any valid member.
    let mut bk_tree = BkTree::new(n);
    for (i, (_, hash)) in ph_pairs.iter().enumerate() {
        bk_tree.insert(i, (*hash).clone());
    }

    // Complete-linkage greedy clustering (same semantics as before):
    // B is added to a cluster only if dist(B, X) ≤ threshold for EVERY existing
    // member X. This guarantees that the worst-case pairwise distance inside any
    // cluster never exceeds the threshold, so the displayed similarity is always
    // ≥ the value the user selected in the slider.
    // The BK-tree narrows the candidate set; the inner loop enforces the
    // complete-linkage constraint — results are identical to the O(n²) version.
    //
    // max_dist is tracked incrementally during formation — no post-loop needed.
    for a in 0..n {
        if ph_grouped[a] { continue; }
        let mut cluster: Vec<usize> = vec![a];
        let mut max_dist = 0u32;

        // BK-tree returns only images within threshold of A — O(log n + k).
        // Sort to preserve ascending-index traversal order (determinism).
        let mut candidates = bk_tree.query(ph_pairs[a].1, phash_threshold);
        candidates.sort_unstable();

        'next_b: for b in candidates {
            if b <= a || ph_grouped[b] { continue; }
            for &x in &cluster {
                let d = ph_pairs[x].1.dist(ph_pairs[b].1);
                if d > phash_threshold {
                    continue 'next_b; // too far from some cluster member — skip
                }
                if d > max_dist { max_dist = d; }
            }
            cluster.push(b);
            ph_grouped[b] = true;
        }

        if cluster.len() < 2 { continue; }
        ph_grouped[a] = true;

        // max_dist ≤ phash_threshold is guaranteed by construction
        let (kind, similarity) = if max_dist == 0 {
            (SimilarityKind::Exact, Some(100u8))
        } else {
            let pct = (((64 - max_dist) as f32 / 64.0) * 100.0).round() as u8;
            (SimilarityKind::Similar, Some(pct))
        };
        let mut entries: Vec<ImageEntry> = cluster.iter()
            .map(|&pos| entry_corrected(ph_pairs[pos].0)).collect();
        mark_original(&mut entries, &retention_rule); sort_by_date(&mut entries);
        for &pos in &cluster { grouped[ph_pairs[pos].0] = true; }
        groups.push(DuplicateGroup { kind, entries, similarity });

        if a % 100 == 0 || a == n.saturating_sub(1) {
            analyze_cb(AnalyzeProgress { analyzed: a + 1, total: n,
                phase: "Comparing similar images…".into() });
        }
    }
    tracing::info!(similar_groups = groups.len(), elapsed_ms = t3c.elapsed().as_millis(), "phase 3 done");
    drop(_p3);

    // ── Phase 4: timestamp tag (fallback) ─────────────────────────────────────
    // Accumulates pHashes for sameDate members with NULL phash in older cache entries.
    let _p4 = tracing::info_span!("phase4_timestamp", records = records.len()).entered();
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
        mark_original(&mut entries, &retention_rule); sort_by_date(&mut entries);
        indices.iter().for_each(|&i| grouped[i] = true);
        samedate_group_indices.push(indices.to_vec());
        groups.push(DuplicateGroup { kind: SimilarityKind::SameDate, entries, similarity });
    }
    tracing::info!(samedate_groups = groups.len() - before, "phase 4 done");
    drop(_p4);

    // Persist on-demand pHashes to cache
    if let Some(ref c) = cache {
        let updates: Vec<(String, String, CachedFile)> = on_demand_phashes.iter()
            .map(|(&i, ph)| {
                let r = &records[i];
                (cache_key(&r.entry.path), r.mtime_key.clone(),
                    build_cached_file(r, Some(phash_to_hex(ph)), None))
            }).collect();
        if !updates.is_empty() {
            tracing::debug!("phase 4: writing {} on-demand pHashes to cache", updates.len());
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
        let _p5 = tracing::info_span!("phase5_cross_group", samedate_groups = samedate_group_indices.len()).entered();
        tracing::debug!("phase 5: cross-group pHash across {} sameDate groups", samedate_group_indices.len());

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

        // Pre-compute pairwise distance matrix for moderate m (same strategy as phase 3c).
        // Entries where either element has no pHash are left as u32::MAX (sentinel = no data).
        let dist_matrix_5: Option<Vec<u32>> = if m > 1 && m <= MATRIX_LIMIT {
            let size = m * (m - 1) / 2;
            let mut mat = vec![u32::MAX; size];
            for a in 0..m {
                if ph_flat[a].is_none() { continue; }
                let base = a * m - a * (a + 1) / 2;
                for b in (a + 1)..m {
                    if let (Some(pa), Some(pb)) = (&ph_flat[a], &ph_flat[b]) {
                        mat[base + (b - a - 1)] = pa.dist(pb);
                    }
                }
            }
            Some(mat)
        } else {
            None
        };

        // Returns the Hamming distance between ph_flat[a] and ph_flat[b], or None if
        // either has no pHash. Uses the pre-computed matrix when available.
        let pair_dist_5 = |a: usize, b: usize| -> Option<u32> {
            let (lo, hi) = if a < b { (a, b) } else { (b, a) };
            if let Some(ref mat) = dist_matrix_5 {
                let d = mat[lo * m - lo * (lo + 1) / 2 + (hi - lo - 1)];
                if d == u32::MAX { None } else { Some(d) }
            } else {
                match (&ph_flat[lo], &ph_flat[hi]) {
                    (Some(pa), Some(pb)) => Some(pa.dist(pb)),
                    _ => None,
                }
            }
        };

        // For each sameDate group: flat indices absorbed into a cross-group cluster.
        let mut absorbed_by_group: Vec<Vec<usize>> =
            vec![Vec::new(); samedate_group_indices.len()];

        let mut new_groups: Vec<DuplicateGroup> = Vec::new();

        for a in 0..m {
            if sd_grouped[a] || ph_flat[a].is_none() { continue; }

            let mut cluster = vec![a];
            // Track seed-to-member max during formation; non-seed pairs are
            // augmented in the post-loop below.
            let mut max_dist = 0u32;

            for b in (a + 1)..m {
                if sd_grouped[b] { continue; }
                if flat[a].1 == flat[b].1 { continue; } // skip same-group pairs
                if let Some(d) = pair_dist_5(a, b) {
                    if d <= min_hamming {
                        if d > max_dist { max_dist = d; }
                        cluster.push(b);
                        sd_grouped[b] = true;
                    }
                }
            }

            if cluster.len() < 2 { continue; }
            // Because same-group pairs are skipped, any accepted b is from a
            // different sameDate group than a. cluster.len() >= 2 therefore
            // already guarantees the cluster spans >= 2 groups — no HashSet needed.

            sd_grouped[a] = true;

            // Augment max_dist with non-seed pairwise distances.
            for x in 1..cluster.len() {
                for y in (x + 1)..cluster.len() {
                    if let Some(d) = pair_dist_5(cluster[x], cluster[y]) {
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
                absorbed_by_group[flat[idx].1].push(idx);
            }

            let mut entries: Vec<ImageEntry> = cluster.iter()
                .map(|&idx| entry_corrected(flat[idx].0))
                .collect();
            mark_original(&mut entries, &retention_rule); sort_by_date(&mut entries);

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

            // Build set of paths absorbed from each sameDate group (HashSet<String>
            // for O(1) membership test during retain).
            let absorbed_paths: Vec<std::collections::HashSet<String>> = (0..num_samedate)
                .map(|g| {
                    absorbed_by_group[g].iter()
                        .map(|&idx| records[flat[idx].0].entry.path.clone())
                        .collect()
                })
                .collect();

            // Vec<bool> instead of HashSet<usize> — group positions are contiguous
            // indices bounded by groups.len(), so a flag array gives O(1) lookup
            // with better cache behaviour.
            let mut remove_flags = vec![false; groups.len()];
            for g in 0..num_samedate {
                if absorbed_paths[g].is_empty() { continue; }
                let group_pos = samedate_start + g;
                if group_pos >= groups.len() { continue; }
                // Remove absorbed entries
                groups[group_pos].entries.retain(|e| !absorbed_paths[g].contains(&e.path));
                // If fewer than 2 members remain, mark for removal
                if groups[group_pos].entries.len() < 2 {
                    remove_flags[group_pos] = true;
                } else {
                    // Re-mark original among remaining entries
                    mark_original(&mut groups[group_pos].entries, &retention_rule);
                }
            }

            let fully_absorbed = remove_flags.iter().filter(|&&f| f).count();
            if fully_absorbed > 0 {
                let mut i = 0usize;
                groups.retain(|_| { let keep = !remove_flags[i]; i += 1; keep });
            }

            tracing::debug!("phase 5: {} cross-group clusters, {} sameDate groups removed, {} partially pruned",
                new_groups.len(),
                fully_absorbed,
                absorbed_paths.iter().filter(|s| !s.is_empty()).count().saturating_sub(fully_absorbed));

            groups.extend(new_groups);
        } else {
            tracing::debug!("phase 5: no cross-group clusters found");
        }
    }

    groups.sort_by(|a, b| {
        a.entries.first().map(|e| e.modified.as_str())
            .cmp(&b.entries.first().map(|e| e.modified.as_str()))
    });

    tracing::info!(
        groups = groups.len(),
        failed_files = failed_files.len(),
        "scan complete"
    );
    Ok((groups, failed_files))
}

/// Re-apply a retention rule to an already-scanned set of groups without a full rescan.
///
/// For each group: clears `is_original` on all entries, then re-marks the one
/// entry that best satisfies `rule`. Called from the `apply_retention_rule` Tauri command.
pub fn apply_retention_rule(mut groups: Vec<DuplicateGroup>, rule: &RetentionRule) -> Vec<DuplicateGroup> {
    for g in &mut groups {
        mark_original(&mut g.entries, rule);
    }
    groups
}
