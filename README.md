# RustyMirror 🔍

> **Vibecoded** from scratch with [Claude](https://claude.ai). Every line of Rust and Vue in this repo was written by an AI assistant through iterative conversation — no human wrote any code directly.

A fast, cross-platform desktop app to find and clean up duplicate and near-duplicate images in your photo library. Built with **Rust + Tauri 2** on the backend and **Vue 3** on the frontend.

---

## ✨ Features

- **Exact duplicates** — detected via Blake3 hash comparison
- **Same-date duplicates** — groups images with matching `YYYYMMDD_HHMMSS` timestamp in the filename
- **Similar images** — perceptual hash (pHash) comparison with adjustable similarity threshold (50–100%)
- **HEIC support** — full thumbnail generation and pHash for Apple HEIC/HEIF files via ImageMagick
- **Network drive support** — works with NAS and mapped network drives, with thumbnail caching to avoid repeated reads
- **Lazy thumbnail loading** — IntersectionObserver-based lazy loading with a global queue (max 4 concurrent Rust calls)
- **Persistent thumbnail cache** — SQLite-backed disk cache + in-memory cache that survives re-scans
- **Incremental scanning** — uses a bulk SQLite query to skip unchanged files (matched by mtime + size)
- **Safe deletion** — local files go to system trash; network files show a clear warning before permanent deletion
- **Scan history** — last 5 scan results cached by directory fingerprint, so re-scanning the same folder is instant
- **Lightbox** — full-screen image viewer with keyboard navigation and thumbnail strip
- **Keyboard navigation** — arrow keys to move between cards and groups, Space to select, Enter to open lightbox

---

## 🛠️ Tech Stack

| Layer | Technology |
|---|---|
| Backend | Rust, Tauri 2 |
| Frontend | Vue 3, Vite, Pinia |
| Hashing | Blake3 (exact), image-hasher pHash (similar) |
| Database | SQLite via rusqlite (hash + thumbnail cache) |
| HEIC conversion | ImageMagick (bundled `magick.exe` on Windows) |
| Package manager | npm |

---

## 🚀 Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) 18+
- [Tauri CLI prerequisites](https://tauri.app/start/prerequisites/) for your platform
- **Windows only**: ImageMagick is bundled — no extra install needed
- **macOS/Linux**: Install ImageMagick via your package manager (`brew install imagemagick` / `apt install imagemagick`)

### Development

```bash
# Install JS dependencies
npm install

# Run in dev mode (with Rust optimizations)
npm run tauri dev -- --release
```

### Production Build

```bash
npm run tauri build
```

The installer will be in `src-tauri/target/release/bundle/`.

---

## 📁 Project Structure

```
rustymirror/
├── src/                        # Vue 3 frontend
│   ├── main.js
│   ├── App.vue
│   ├── store/
│   │   ├── scan.js             # Main state: scan, groups, thumbnails, history
│   │   └── history.js          # Persistent scan history (tauri-plugin-store)
│   ├── utils/logger.js
│   ├── styles/
│   │   ├── tokens.css
│   │   └── base.css
│   └── components/
│       ├── Sidebar.vue         # Folders, similarity slider, filters, history
│       ├── ResultsArea.vue     # Action bar, groups list, delete dialog
│       ├── ImageGroup.vue      # Card grid with lazy thumbnails
│       ├── Lightbox.vue        # Full-screen viewer
│       └── ScanProgress.vue    # Phase-aware progress bar with ETA
└── src-tauri/                  # Rust backend
    ├── Cargo.toml
    ├── tauri.conf.json
    └── src/
        ├── main.rs
        ├── lib.rs
        ├── types.rs
        ├── hasher.rs           # pHash with 64x64 pre-resize (~100x speedup)
        ├── heic.rs             # HEIC→JPEG conversion via ImageMagick
        ├── scanner.rs          # 4-phase scan: exact → sameDate → HEIC → similar
        ├── commands.rs         # Tauri commands: scan, delete, thumbnail, open
        └── cache.rs            # SQLite cache: hash records + thumbnails
```

---

## ⚙️ How It Works

### Scan Phases

1. **Phase 1 — Index**: Bulk-loads all cached records from SQLite in a single query. Only files with changed `mtime` or `size` are re-hashed from disk.
2. **Phase 2 — Exact**: Groups files by identical Blake3 hash.
3. **Phase 3 — Same date**: Groups files sharing the same `YYYYMMDD_HHMMSS` timestamp pattern in their filename.
4. **Phase 4 — Similar**: Generates pHash for all non-HEIC images (HEIC pHashes are cached from previous scans), then runs O(n²) Hamming distance comparison with the configured threshold.

### Thumbnail Pipeline

- **Local JPG/PNG**: Served directly via Tauri's asset protocol (`convertFileSrc`) — zero Rust involvement.
- **HEIC files**: Converted to JPEG by ImageMagick, cached to `thumb_cache/<blake3[:16]>.jpg`.
- **Network JPG/PNG**: Read and resized by Rust, cached to `thumb_cache/jpg_<blake3[:16]>.jpg` to avoid repeated network reads.
- In-memory `thumbCache` in Pinia persists across re-scans for instant display.

### Similarity Threshold

The UI slider (50–100%) maps to a Hamming distance on the 64-bit pHash:

```
hamming_threshold = round((100 - similarity%) / 100 * 64)
```

At 90% similarity, files with a Hamming distance ≤ 6 are considered similar.

---

## 🗑️ Deletion Behavior

| File location | Behavior |
|---|---|
| Local drive | Moved to system trash (recoverable) |
| Network drive | **Permanently deleted** — dialog shows explicit warning |
| Mixed selection | Both behaviors apply — dialog shows color-coded indicators |

Deletion is handled via PowerShell on Windows to correctly handle Unicode paths and network UNC paths.

---

## 📝 Version

Current version: **1.0.0**

---

## ⚠️ Disclaimer

This app deletes files. Always verify your selection before confirming deletion. The authors take no responsibility for accidental data loss. When in doubt, use the local drive path — files will go to the trash and can be recovered.

---

## 🤖 About the Vibe

This entire project was built through conversation with Claude (Anthropic). The human provided direction, tested the app, shared logs, and described what they wanted. The AI wrote every file. It's an experiment in what "vibecoding" looks like for a non-trivial desktop application with a compiled systems language backend.
