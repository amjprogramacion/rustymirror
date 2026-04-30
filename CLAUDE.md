# RustyMirror

Tauri v2 desktop app for duplicate image detection, metadata management, and media organization.

- **Backend:** Rust (`src-tauri/src/`)
- **Frontend:** Vue 3 + Vite (`src/`)

## Backend structure

| File/dir | Purpose |
|---|---|
| `commands/` | Tauri commands split by domain (see below) |
| `types.rs` | Shared serialisable types (`ImageEntry`, `DuplicateGroup`, `RetentionRule`, etc.) |
| `scanner/mod.rs` | `find_duplicates()` — 5-phase scan pipeline |
| `scanner/walk.rs` | `collect_images()` — filesystem traversal |
| `scanner/bktree.rs` | BK-tree for Hamming-distance pHash deduplication |
| `scanner/grouping.rs` | `mark_original()`, `sort_by_date()` |
| `scanner/record.rs` | `FileRecord`, hashing helpers, cache key logic |
| `cache.rs` | SQLite (rusqlite) cache — blake3, phash, fast_phash, blur_score |
| `hasher.rs` | Perceptual hash generation |
| `heic.rs` | HEIC/HEIF → JPEG conversion via `magick.exe` sidecar |
| `metadata.rs` | EXIF read/write via ExifTool |
| `exiftool.rs` | ExifTool sidecar — path resolution, single-file/batch JSON I/O, daemon (`-stay_open`) |
| `organizer.rs` | Media organizer — date-smart renaming, optional move to structured directories |

### commands/ submodules

| File | Commands |
|---|---|
| `mod.rs` | Shared state (`ScanState`, `MetaScanState`, `OrganizerState`, `FileListCache`), `AppError`, shared helpers |
| `scan.rs` | `scan_directories`, `directory_fingerprint`, `apply_retention_rule_cmd`, `stop_scan`, `stop_meta_scan` |
| `metadata.rs` | `scan_for_metadata`, `read_metadata`, `write_metadata` |
| `thumbnail.rs` | `get_thumbnail`, `get_full_image` |
| `file.rs` | `delete_files`, `open_file`, `open_folder`, `is_network_path`, `check_paths_exist`, `log_message` |
| `cache.rs` | `get_cache_size`, `clear_cache`, `flush_cache`, `get_thumb_cache_size`, `clear_thumb_cache`, `is_debug_build` |
| `organizer.rs` | `preview_organize`, `execute_organize`, `preview_rewrite_date`, `execute_metadata_rewrite`, `stop_organize` |
| `media.rs` | `count_media_files` |

## Scan pipeline (5 phases inside `find_duplicates`)
1. File enumeration (`collect_images`)
2. Blake3 hashing + SQLite cache lookup
3. Exact duplicate grouping
4. Perceptual hash analysis (BK-tree, Hamming threshold default 6)
5. Cross-date pHash comparison (optional, O(n²), capped at 4 000 files)

## Frontend structure

| Path | Purpose |
|---|---|
| `src/store/duplicates.js` | Duplicate detection state |
| `src/store/metadata.js` | Metadata editor state |
| `src/store/organizer.js` | Organizer state |
| `src/store/mapView.js` | Two-slot GPS map state (edit + preview) |
| `src/store/panel.js` | Image detail panel state |
| `src/store/thumbnails.js` | Thumbnail cache |
| `src/store/historyFactory.js` | Shared undo/redo factory |
| `src/store/duplicatesHistory.js` | Undo/redo for duplicates mode |
| `src/store/metadataHistory.js` | Undo/redo for metadata mode |
| `src/store/organizerHistory.js` | Undo/redo for organizer mode |
| `src/composables/` | `useMode`, `useSettings`, `useGpsEditor`, `useCacheSize`, `useUpdater`, `useFolderPicker` |
| `src/components/` | `DuplicatesView`, `MetadataView`, `OrganizerView`, `Lightbox`, `MapPreview`, `ImageDetailPanel`, `BatchEditPanel`, `FailedFilesWarning`, `SidebarHistory`, `SidebarMeta`, `SidebarOrganizer`, panel section components |
| `src/utils/` | `errors.js`, `formatters.js`, `logger.js` |

## Key conventions

- **Error type:** `AppError` enum in `commands/mod.rs`, serialised as `{ "type": "...", "message": "..." }` so the frontend can branch on `type`. All Tauri commands return `Result<T, AppError>`.
- **Serde:** all public types use `#[serde(rename_all = "camelCase")]`.
- **Cache invalidation:** call `cache.evict_deleted(&paths)` after any file write/delete.
- **Metadata writes:** always write to a temp file then rename (atomic, prevents partial writes).
- **Debug vs release cache:** `cache_data_dir()` appends `/dev` in debug builds to keep caches separate.
- **File deletion:** UNC/network paths (`\\` or `//`) are deleted permanently; local paths go to the OS recycle bin via the `trash` crate.
- **NAS double-traversal:** `FileListCache` state lets `scan_directories` reuse the file list already enumerated by `directory_fingerprint`, avoiding a second SMB walk.
- **ExifTool daemon:** `ExifToolDaemon` in `exiftool.rs` keeps a single `-stay_open` process alive per scan session. Stdin is written from a background thread to avoid pipe-buffer deadlocks.

## Tauri events emitted from backend
- `scan_progress` → `ScanProgress { scanned, total }`
- `analyze_progress` → `AnalyzeProgress { analyzed, total, phase }`
- `delete_progress` → `{ done, total }`
- `meta_scan_progress` → `MetaScanProgress { total, processed }`
- `meta_analyze_progress` → `AnalyzeProgress { analyzed, total, phase }`
- `organize_progress` → organizer job progress

## Sidecar binaries

`magick.exe` and `exiftool.exe` are **not committed to git** — they are downloaded at CI build time and kept locally via `.gitignore`. See `.github/workflows/release.yml` for the download steps. Fresh clones require manual download of both binaries into `src-tauri/resources/`.

- `exiftool.exe` + `exiftool_files/` — ExifTool 13.57 (Perl runtime included, always required alongside)
- `magick.exe` — ImageMagick 7.1.2 portable Q16 x64 (single standalone exe)

`tauri.conf.json` uses `"resources/**/*"` to recursively bundle `exiftool_files/` subdirectories.

## Common commands
```bash
cargo tauri dev                                      # dev server
cargo test --manifest-path src-tauri/Cargo.toml     # backend tests
npm run build                                        # frontend build
npm run tauri build                                  # release build
```
