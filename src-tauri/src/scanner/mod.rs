mod bktree;
mod grouping;
mod record;
mod walk;

use std::collections::{HashMap, HashSet};
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
    // `path_cache_key` is pre-computed by the caller (already normalised in
    // `path_strings`) — avoids a redundant NFC+lowercase pass per file.
    let process_one = |path: &PathBuf, path_cache_key: &str| -> Option<FileRecord> {
        if stop_phase1.load(AOrdering::Relaxed) { return None; }

        let path_str = path.to_string_lossy().to_string();

        let meta = std::fs::metadata(path).ok()?;
        let size  = meta.len();
        let mtime = mtime_rfc3339(&meta);

        let record = if let Some(entry) = bulk_cache.get(path_cache_key) {
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

        let folder_results: Vec<Option<FileRecord>> = (start..start + len)
            .into_par_iter()
            .map(|idx| process_one(&paths[idx], &path_strings[idx]))
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

    // Internal accumulator: stores record indices only during phases 2-5.
    // ImageEntry is cloned exactly once per group member in the final conversion
    // pass, after phase 5 has pruned any absorbed sameDate members — avoiding
    // allocations for entries that would otherwise be discarded.
    struct GroupBuild {
        kind:       SimilarityKind,
        indices:    Vec<usize>,    // indices into `records`, sorted by modified date
        similarity: Option<u8>,
    }
    let mut groups: Vec<GroupBuild> = Vec::new();

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
        indices.iter().for_each(|&i| grouped[i] = true);
        groups.push(GroupBuild { kind: SimilarityKind::Exact, indices: indices.to_vec(), similarity: Some(100) });
    }
    tracing::info!(exact_groups = groups.len(), "phase 2 done");
    drop(_p2);

    // ── Phase 3: perceptual hash (user threshold) ─────────────────────────────
    if stop.load(AOrdering::Relaxed) {
        tracing::debug!("scan stopped by user before phase 3");
        // HEIC extras not yet computed — use raw record entries directly.
        let partial: Vec<DuplicateGroup> = groups.into_iter().map(|gb| {
            let mut entries: Vec<ImageEntry> = gb.indices.iter()
                .map(|&i| records[i].entry.clone()).collect();
            mark_original(&mut entries, &retention_rule);
            sort_by_date(&mut entries);
            DuplicateGroup { kind: gb.kind, entries, similarity: gb.similarity }
        }).collect();
        return Ok((partial, failed_files));
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

    // Note: HEIC dimension/date corrections for exact-duplicate groups (phase 2)
    // are applied by `entry_corrected` in the final conversion pass — no need
    // to patch groups[].entries here since GroupBuild stores only indices.

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
    let mut last_ph_progress = std::time::Instant::now();

    // Fingerprint: blake3 over every (path, phash_bytes) in ph_pairs order.
    // A fingerprint match means the exact same ordered set of hashes will be
    // inserted into the tree, so the stored ph_idx values remain valid.
    let bktree_fingerprint = {
        let mut h = blake3::Hasher::new();
        for &(i, ph) in &ph_pairs {
            h.update(records[i].entry.path.as_bytes());
            h.update(ph.as_bytes());
        }
        h.finalize().to_hex().to_string()
    };

    // Try to load a previously serialised BK-tree for this exact hash set.
    // Fall back to building from scratch if absent or corrupt.
    let cached_blob = cache.as_ref()
        .and_then(|c| c.load_bktree_blob(&bktree_fingerprint, fast_mode));
    let tree_from_cache = cached_blob.is_some();

    // Build a BK-tree for O(n log n) candidate lookup instead of O(n²) scan.
    // For each ungrouped image A, the tree returns only the images within
    // `phash_threshold` Hamming distance — skipping the rest entirely.
    // Since A is always in its own cluster, complete-linkage requires every
    // member to be within threshold of A, so querying on A's hash yields the
    // complete candidate set without missing any valid member.
    let bk_tree = if let Some(tree) = cached_blob.as_deref()
        .and_then(|b| BkTree::deserialize(b))
        .filter(|t| t.nodes.len() == n)
    {
        tracing::debug!("phase 3c: BK-tree loaded from cache ({} nodes)", n);
        tree
    } else {
        let mut t = BkTree::new(n);
        for (i, (_, hash)) in ph_pairs.iter().enumerate() {
            t.insert(i, (*hash).clone());
        }
        t
    };

    // Persist the tree if it was freshly built.
    if !tree_from_cache {
        if let Some(ref c) = cache {
            c.save_bktree_blob(&bktree_fingerprint, fast_mode, &bk_tree.serialize());
            tracing::debug!("phase 3c: BK-tree saved to cache ({} nodes)", n);
        }
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
        let g_indices: Vec<usize> = cluster.iter().map(|&pos| ph_pairs[pos].0).collect();
        for &pos in &cluster { grouped[ph_pairs[pos].0] = true; }
        groups.push(GroupBuild { kind, indices: g_indices, similarity });

        let now = std::time::Instant::now();
        if now.duration_since(last_ph_progress).as_millis() >= 100 || a == n.saturating_sub(1) {
            last_ph_progress = now;
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

    // `before` marks where sameDate groups start in `groups`; phase 5 reads
    // groups[before..].indices directly — no separate samedate_group_indices needed.
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

        indices.iter().for_each(|&i| grouped[i] = true);
        groups.push(GroupBuild { kind: SimilarityKind::SameDate, indices: indices.to_vec(), similarity });
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
    let num_samedate = groups.len() - before;
    if cross_date_phash && num_samedate >= 2 {
        let _p5 = tracing::info_span!("phase5_cross_group", samedate_groups = num_samedate).entered();
        tracing::debug!("phase 5: cross-group pHash across {} sameDate groups", num_samedate);

        let min_hamming: u32 = 16;

        // flat[i] = (record_idx, samedate_group_idx)
        // Built directly from groups[before..].indices — no separate Vec needed.
        let mut flat: Vec<(usize, usize)> = Vec::new();
        for (g, gb) in groups[before..].iter().enumerate() {
            for &r in &gb.indices {
                flat.push((r, g));
            }
        }

        let ph_flat: Vec<Option<image_hasher::ImageHash>> = flat.iter()
            .map(|&(ri, _)| resolve_phash_owned(ri, &records, &heic_extra, &mut on_demand_phashes, fast_mode))
            .collect();

        let m = flat.len();
        let mut sd_grouped = vec![false; m];

        // Build a BK-tree for O(n log n) cross-group candidate lookup,
        // mirroring phase 3. Avoids the O(n²) distance precomputation.
        let mut sd_tree = BkTree::new(m);
        for (i, ph) in ph_flat.iter().enumerate() {
            if let Some(hash) = ph { sd_tree.insert(i, hash.clone()); }
        }

        // For each sameDate group: set of absorbed record indices.
        // HashSet<usize> avoids the previous HashSet<String> (path clones).
        let mut absorbed_by_group: Vec<HashSet<usize>> = vec![HashSet::new(); num_samedate];

        let mut new_groups: Vec<GroupBuild> = Vec::new();

        for a in 0..m {
            if sd_grouped[a] || ph_flat[a].is_none() { continue; }

            let mut cluster = vec![a];
            // Track seed-to-member max during formation; non-seed pairs are
            // augmented in the post-loop below.
            let mut max_dist = 0u32;

            // BK-tree returns only images within min_hamming of seed — O(log n + k).
            let mut candidates = sd_tree.query(ph_flat[a].as_ref().unwrap(), min_hamming);
            candidates.sort_unstable();

            for b in candidates {
                if b <= a || sd_grouped[b] { continue; }
                if flat[a].1 == flat[b].1 { continue; } // skip same-group pairs
                // ph_flat[b] is Some since it was inserted into the BK-tree.
                let d = ph_flat[a].as_ref().unwrap().dist(ph_flat[b].as_ref().unwrap());
                if d > max_dist { max_dist = d; }
                cluster.push(b);
                sd_grouped[b] = true;
            }

            if cluster.len() < 2 { continue; }
            // Because same-group pairs are skipped, any accepted b is from a
            // different sameDate group than a. cluster.len() >= 2 therefore
            // already guarantees the cluster spans >= 2 groups — no HashSet needed.

            sd_grouped[a] = true;

            // Augment max_dist with non-seed pairwise distances.
            for x in 1..cluster.len() {
                for y in (x + 1)..cluster.len() {
                    if let (Some(pa), Some(pb)) = (&ph_flat[cluster[x]], &ph_flat[cluster[y]]) {
                        let d = pa.dist(pb);
                        if d > max_dist { max_dist = d; }
                    }
                }
            }

            let similarity = if max_dist == 0 {
                Some(100u8)
            } else {
                Some((((64 - max_dist) as f32 / 64.0) * 100.0).round() as u8)
            };

            // Record absorbed record indices per source group for later pruning.
            for &idx in &cluster {
                absorbed_by_group[flat[idx].1].insert(flat[idx].0);
            }

            // Store record indices only — ImageEntry cloned in the final pass.
            let g_indices: Vec<usize> = cluster.iter().map(|&idx| flat[idx].0).collect();

            // Cross-group clusters are labelled SameDate with a similarity score
            // so the user can see they were linked by date AND visual similarity.
            new_groups.push(GroupBuild { kind: SimilarityKind::SameDate, indices: g_indices, similarity });
        }

        if !new_groups.is_empty() {
            // Rebuild sameDate groups: remove absorbed members, drop groups with < 2 left.
            // groups[before + g] corresponds to the g-th sameDate group.
            let samedate_start = before;

            // Vec<bool> instead of HashSet<usize> — group positions are contiguous
            // indices bounded by groups.len(), so a flag array gives O(1) lookup
            // with better cache behaviour.
            let mut remove_flags = vec![false; groups.len()];
            for g in 0..num_samedate {
                if absorbed_by_group[g].is_empty() { continue; }
                let group_pos = samedate_start + g;
                if group_pos >= groups.len() { continue; }
                // Remove absorbed record indices — O(1) per member, no string comparison.
                groups[group_pos].indices.retain(|i| !absorbed_by_group[g].contains(i));
                // If fewer than 2 members remain, mark for removal.
                if groups[group_pos].indices.len() < 2 {
                    remove_flags[group_pos] = true;
                }
                // mark_original is deferred to the final conversion pass.
            }

            let fully_absorbed = remove_flags.iter().filter(|&&f| f).count();
            if fully_absorbed > 0 {
                let mut i = 0usize;
                groups.retain(|_| { let keep = !remove_flags[i]; i += 1; keep });
            }

            tracing::debug!("phase 5: {} cross-group clusters, {} sameDate groups removed, {} partially pruned",
                new_groups.len(),
                fully_absorbed,
                absorbed_by_group.iter().filter(|s| !s.is_empty()).count().saturating_sub(fully_absorbed));

            groups.extend(new_groups);
        } else {
            tracing::debug!("phase 5: no cross-group clusters found");
        }
    }

    // ── Final conversion: GroupBuild → DuplicateGroup ─────────────────────────
    // Exactly one ImageEntry clone per group member, after all phases have
    // settled the final member sets. mark_original and sort_by_date are applied
    // here so they see the corrected dimensions and the definitive membership.
    let mut final_groups: Vec<DuplicateGroup> = groups.into_iter().map(|gb| {
        let mut entries: Vec<ImageEntry> = gb.indices.iter()
            .map(|&i| entry_corrected(i)).collect();
        mark_original(&mut entries, &retention_rule);
        sort_by_date(&mut entries);
        DuplicateGroup { kind: gb.kind, entries, similarity: gb.similarity }
    }).collect();

    final_groups.sort_by(|a, b| {
        a.entries.first().map(|e| e.modified.as_str())
            .cmp(&b.entries.first().map(|e| e.modified.as_str()))
    });

    tracing::info!(
        groups = final_groups.len(),
        failed_files = failed_files.len(),
        "scan complete"
    );
    Ok((final_groups, failed_files))
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
