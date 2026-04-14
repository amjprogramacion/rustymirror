# RustyMirror

Tauri desktop app (v1.3.x) for duplicate image detection and metadata management.

- **Backend:** Rust (`src-tauri/src/`)
- **Frontend:** Vue 3 + Vite (`src/`)

## Backend structure

| File/dir | Purpose |
|---|---|
| `commands.rs` | All Tauri commands — single source of the frontend/backend contract |
| `types.rs` | Shared serialisable types (`ImageEntry`, `DuplicateGroup`, `RetentionRule`, etc.) |
| `scanner/mod.rs` | `find_duplicates()` — 5-phase scan pipeline |
| `scanner/walk.rs` | `collect_images()` — filesystem traversal |
| `scanner/bktree.rs` | BK-tree for Hamming-distance pHash deduplication |
| `scanner/grouping.rs` | `mark_original()`, `sort_by_date()` |
| `scanner/record.rs` | `FileRecord`, hashing helpers, cache key logic |
| `cache.rs` | SQLite (rusqlite) cache — blake3, phash, fast_phash, blur_score |
| `hasher.rs` | Perceptual hash generation |
| `heic.rs` | HEIC/HEIF → JPEG conversion via external tools |
| `metadata.rs` | EXIF read/write |

## Scan pipeline (5 phases inside `find_duplicates`)
1. File enumeration (`collect_images`)
2. Blake3 hashing + SQLite cache lookup
3. Exact duplicate grouping
4. Perceptual hash analysis (BK-tree, Hamming threshold default 6)
5. Cross-date pHash comparison (optional, O(n²), capped at 4 000 files)

## Frontend structure

| Path | Purpose |
|---|---|
| `src/store/` | Pinia stores: `duplicates`, `metadata`, `mapView`, `panel`, `thumbnails` |
| `src/composables/` | `useMode`, `useSettings`, `useGpsEditor`, `useCacheSize`, `useUpdater` |
| `src/components/` | `DuplicatesView`, `MetadataView`, `Lightbox`, `MapPreview`, `ImageDetailPanel`, etc. |
| `src/utils/` | `errors.js`, `formatters.js`, `logger.js` |

## Key conventions

- **Error type:** `AppError` enum in `commands.rs`, serialised as `{ "type": "...", "message": "..." }` so the frontend can branch on `type`. All Tauri commands return `Result<T, AppError>`.
- **Serde:** all public types use `#[serde(rename_all = "camelCase")]`.
- **Cache invalidation:** call `cache.evict_deleted(&paths)` after any file write/delete.
- **Metadata writes:** always write to a temp file then rename (atomic, prevents partial writes).
- **Debug vs release cache:** `cache_data_dir()` appends `/dev` in debug builds to keep caches separate.
- **File deletion:** UNC/network paths (`\\` or `//`) are deleted permanently; local paths go to the OS recycle bin via the `trash` crate.
- **NAS double-traversal:** `FileListCache` state lets `scan_directories` reuse the file list already enumerated by `directory_fingerprint`, avoiding a second SMB walk.

## Tauri events emitted from backend
- `scan_progress` → `ScanProgress { scanned, total }`
- `analyze_progress` → `AnalyzeProgress { analyzed, total, phase }`
- `delete_progress` → `{ done, total }`

## Common commands
```bash
cargo tauri dev                                      # dev server
cargo test --manifest-path src-tauri/Cargo.toml     # backend tests
npm run build                                        # frontend build
```
