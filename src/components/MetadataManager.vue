<template>
<div class="meta-manager-root">
  <!-- Scanning / geocoding modal -->
  <div class="scan-overlay" v-if="meta.scanning || (meta.geocoding && prefetchFilters)">
    <div class="scan-card">
      <div class="spinner" />
      <template v-if="meta.scanning && !meta.geocoding">
        <p class="scan-title">Scanning images…</p>
        <div class="bar-track"><div class="bar-indeterminate" /></div>
      </template>
      <template v-else>
        <p class="scan-title">Fetching locations…</p>
        <p class="scan-subtitle">{{ meta.images.length.toLocaleString() }} images found</p>
        <div class="bar-track"><div class="bar-indeterminate" /></div>
      </template>
    </div>
  </div>

  <!-- Empty state -->
  <div class="empty-state" v-else-if="!meta.scanDone">
    <p>Add folders and press <strong>Scan</strong> to load images.</p>
  </div>

  <!-- Results -->
  <template v-else>
    <!-- Action bar -->
    <div class="action-bar">
      <button
        class="btn btn-ghost"
        :class="{ active: meta.multiSelect }"
        @click="toggleMultiSelect"
      >Multi-select</button>
      <button
        class="btn btn-ghost"
        :disabled="meta.selectedCount === 0"
        @click="meta.clearSelection(); meta.multiSelect = false"
      >Deselect all</button>
      <button
        class="btn btn-edit-exif"
        :disabled="meta.selectedCount < 2"
        @click="openBatchEdit"
      >Edit EXIF</button>

      <span class="image-count">{{ meta.filteredImages.length }} image{{ meta.filteredImages.length !== 1 ? 's' : '' }}</span>

      <!-- Search -->
      <div class="search-wrap">
        <span class="search-icon">⌕</span>
        <input
          class="search-input"
          type="text"
          placeholder="Search by filename…"
          v-model="meta.searchQuery"
          @keydown.escape="meta.searchQuery = ''"
        />
        <button
          v-if="meta.searchQuery"
          class="search-clear"
          @click="meta.searchQuery = ''"
          title="Clear search"
        >✕</button>
      </div>
    </div>

    <!-- Grid -->
    <div class="grid-scroll" ref="gridEl">
      <div v-if="meta.filteredImages.length === 0" class="no-results">
        <template v-if="meta.searchQuery">
          No images match <em>"{{ meta.searchQuery }}"</em>.
        </template>
        <template v-else>No images found.</template>
      </div>

      <div class="cards-grid">
        <div
          v-for="(entry, idx) in meta.filteredImages"
          :key="entry.path"
          class="card"
          :class="{ selected: meta.selected.has(entry.path) }"
          :tabindex="0"
          :data-card-path="entry.path"
          @click="onCardClick(entry, idx)"
        >
          <div class="thumb-wrap" :data-path="entry.path">
            <img
              v-if="directSrcCache[entry.path]"
              :src="directSrcCache[entry.path]"
              class="thumb"
              draggable="false"
            />
            <img
              v-else-if="thumbCache[entry.path] && thumbCache[entry.path] !== THUMB_ERROR"
              :src="thumbCache[entry.path]"
              class="thumb"
              draggable="false"
            />
            <div v-else-if="thumbCache[entry.path] === THUMB_ERROR" class="thumb-placeholder">
              <span class="thumb-ext">{{ fileExt(entry.path).toUpperCase() }}</span>
              <span class="thumb-no-preview">No preview</span>
            </div>
            <div v-else class="thumb-placeholder">
              <span class="thumb-loading" />
            </div>
            <div class="selected-overlay" v-if="meta.selected.has(entry.path)">
              <span class="checkmark">&#10003;</span>
            </div>
          </div>

          <div class="meta">
            <p class="meta-name" :title="entry.path">{{ fileName(entry.path) }}</p>
            <p class="meta-detail">
              {{ entry.width > 0 ? `${entry.width}x${entry.height}` : '--' }} · {{ formatSize(entry.sizeBytes) }}
            </p>
            <p class="meta-detail">{{ formatDate(entry.dateTaken ?? entry.modified) }}</p>
            <div class="meta-actions">
              <div class="btn-group">
                <button class="btn-open btn-explore" @click.stop="openFolder(entry.path)" title="Show in folder">Explore</button>
                <button class="btn-open" @click.stop="openFile(entry.path)" title="Open file">Open</button>
              </div>
              <label v-if="!meta.multiSelect" class="card-checkbox" @click.stop>
                <input
                  type="checkbox"
                  :checked="meta.selected.has(entry.path)"
                  @change="meta.toggleSelected(entry.path)"
                />
              </label>
            </div>
          </div>
        </div>
      </div>
    </div>
  </template>

  <MetadataBottomPanel />
</div>
</template>

<script setup>
import { ref, computed, onBeforeUnmount, watch, nextTick } from 'vue'
import { convertFileSrc, invoke } from '@tauri-apps/api/core'
import { useMetadataStore } from '../store/metadata'
import { useScanStore } from '../store/scan'
import { useSettings } from '../composables/useSettings'
import { fileExt, fileName, formatSize, formatDate } from '../utils/formatters'
import MetadataBottomPanel from './MetadataBottomPanel.vue'

const meta      = useMetadataStore()
const scanStore = useScanStore()
const { prefetchFilters } = useSettings()
const gridEl    = ref(null)
const THUMB_ERROR = '__error__'
const HEIC_EXTS   = new Set(['heic', 'heif'])

// Thumbnails are read/written through the scan store so the cache is shared
// between duplicate finder and metadata editor — no redundant re-downloads.
const thumbCache     = scanStore.thumbCache
const directSrcCache = scanStore.directSrcCache

function needsRust(path) {
  return HEIC_EXTS.has(fileExt(path)) || meta.isNetworkPath(path)
}

// ── Thumbnail lazy loading ────────────────────────────────────────────────────
let observer = null

function setupObserver(root) {
  observer?.disconnect()

  observer = new IntersectionObserver((entries) => {
    for (const e of entries) {
      const path = e.target.dataset.cardPath
      if (!path) continue
      if (path in thumbCache || path in directSrcCache) {
        observer.unobserve(e.target)
        continue
      }
      if (e.isIntersecting) {
        if (needsRust(path)) {
          scanStore.enqueueThumbnail(path)
        } else {
          scanStore.setDirectSrc(path, convertFileSrc(path))
          observer.unobserve(e.target)
        }
      } else {
        scanStore.dequeueThumbnail(path)
      }
    }
  }, { root, rootMargin: '400px', threshold: 0 })

  root.querySelectorAll('.card[data-card-path]').forEach(el => {
    const path = el.dataset.cardPath
    if (path && !(path in thumbCache) && !(path in directSrcCache)) observer.observe(el)
  })
}

function toggleMultiSelect() {
  const enabling = !meta.multiSelect
  meta.multiSelect = enabling
  if (enabling) {
    const panel = scanStore.metadataPanel
    if (panel && !panel.batch && panel.entry?.path) {
      meta.toggleSelected(panel.entry.path)
    }
  }
}

function openBatchEdit() {
  const selected = meta.filteredImages.filter(e => meta.selected.has(e.path))
  if (selected.length < 2) return
  scanStore.openBatchEditPanel(selected)
}

// Reactive key that changes whenever the selected set changes (add, remove, swap)
const selectedKey = computed(() => {
  const arr = []
  meta.selected.forEach(p => arr.push(p))
  return arr.sort().join('|')
})

// While the batch panel is open, keep it in sync with selection changes
watch(selectedKey, () => {
  const panel = scanStore.metadataPanel
  if (!panel || panel.saving) return

  const count = meta.selectedCount
  if (count === 0) {
    scanStore.closeMetadataPanel()
  } else if (count === 1) {
    if (panel.batch) {
      const entry = meta.filteredImages.find(e => meta.selected.has(e.path))
      if (entry) scanStore.openMetadataPanel(entry)
    }
  } else {
    const entries = meta.filteredImages.filter(e => meta.selected.has(e.path))
    scanStore.openBatchEditPanel(entries)
  }
})

// Close the metadata panel on sort, filter, or new scan
watch(() => [meta.sortBy, meta.sortDir], () => scanStore.closeMetadataPanel())
watch(() => [meta.filterDateFrom, meta.filterDateTo, meta.filterLocation, meta.filterDevice], () => scanStore.closeMetadataPanel())
watch(() => meta.scanning, (scanning) => { if (scanning) scanStore.closeMetadataPanel() })

// When visible images change (filter/sort), cancel pending thumb loads and
// re-register the observer so newly visible cards get prioritised.
watch(
  () => meta.filteredImages,
  () => {
    scanStore.clearThumbQueue()
    nextTick(() => { if (gridEl.value) setupObserver(gridEl.value) })
  }
)

// Watch the ref directly: fires the moment the v-else block mounts and
// assigns gridEl. requestAnimationFrame ensures layout is complete before
// we query card positions.
watch(gridEl, (el) => {
  if (el) requestAnimationFrame(() => setupObserver(el))
  else observer?.disconnect()
})

onBeforeUnmount(() => observer?.disconnect())

// ── Card interaction ──────────────────────────────────────────────────────────
function onCardClick(entry, idx) {
  if (meta.multiSelect) {
    meta.toggleSelected(entry.path)
  } else {
    scanStore.openMetadataPanel(entry)
  }
}

// ── File actions ──────────────────────────────────────────────────────────────
async function openFile(path)   { await invoke('open_file',   { path }) }
async function openFolder(path) { await invoke('open_folder', { path }) }
</script>

<style scoped>
/* ── States ── */
.scan-overlay {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--bg-primary);
}

.scan-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-6);
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-lg);
  width: 320px;
}

.scan-title {
  font-size: var(--font-size-md);
  font-weight: 500;
  color: var(--text-primary);
  text-align: center;
}

.scan-subtitle {
  font-size: var(--font-size-sm);
  color: var(--text-muted);
}

.bar-track {
  width: 100%;
  height: 6px;
  background: var(--bg-secondary);
  border-radius: var(--border-radius-pill);
  overflow: hidden;
}

@keyframes indeterminate {
  0%   { transform: translateX(-100%); }
  100% { transform: translateX(400%); }
}
.bar-indeterminate {
  height: 100%;
  width: 25%;
  background: var(--color-accent);
  border-radius: var(--border-radius-pill);
  animation: indeterminate 1.4s ease-in-out infinite;
}

.empty-state {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-muted);
  font-size: var(--font-size-md);
}

@keyframes spin { to { transform: rotate(360deg); } }
.spinner {
  width: 32px; height: 32px;
  border: 3px solid var(--border-color);
  border-top-color: var(--color-accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

/* ── Action bar ── */
.action-bar {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: 0 var(--space-4);
  height: 44px;
  border-bottom: 1px solid var(--border-color);
  background: var(--bg-secondary);
  flex-shrink: 0;
}

.btn {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  padding: 5px var(--space-3);
  border-radius: var(--border-radius-md);
  font-size: var(--font-size-sm);
  font-weight: 500;
  transition: background var(--transition), opacity var(--transition);
  white-space: nowrap;
}
.btn-ghost { background: transparent; color: var(--text-secondary); border: 1px solid var(--border-color); }
.btn-ghost:hover:not(:disabled) { background: var(--bg-card); }
.btn-ghost.active { background: var(--color-accent); color: #fff; border-color: var(--color-accent); }
.btn:disabled { opacity: 0.4; cursor: not-allowed; }

.image-count {
  font-size: var(--font-size-xs);
  color: var(--text-muted);
  margin-left: var(--space-1);
}

/* ── Search ── */
.search-wrap {
  margin-left: auto;
  position: relative;
  display: flex;
  align-items: center;
}

.search-icon {
  position: absolute;
  left: 8px;
  font-size: 15px;
  color: var(--text-muted);
  pointer-events: none;
  line-height: 1;
}

.search-input {
  width: 200px;
  padding: 4px 28px 4px 26px;
  border-radius: var(--border-radius-md);
  border: 1px solid var(--border-color);
  background: var(--bg-card);
  color: var(--text-primary);
  font-size: var(--font-size-sm);
  transition: border-color var(--transition), width var(--transition);
  outline: none;
}
.search-input::placeholder { color: var(--text-muted); }
.search-input:focus { border-color: var(--color-accent); width: 260px; }

.search-clear {
  position: absolute;
  right: 6px;
  background: none;
  color: var(--text-muted);
  font-size: 11px;
  padding: 2px 4px;
  line-height: 1;
  border-radius: 3px;
  transition: color var(--transition);
}
.search-clear:hover { color: var(--text-primary); }

/* ── Grid scroll ── */
.grid-scroll {
  flex: 1;
  overflow-y: auto;
  padding: var(--space-4) var(--space-4) 32px var(--space-4);
}

.no-results {
  color: var(--text-muted);
  text-align: center;
  margin-top: var(--space-6);
}

.cards-grid {
  display: grid;
  grid-template-columns: repeat(8, 1fr);
  gap: var(--space-3);
  will-change: scroll-position;
}

/* ── Card ── */
.card {
  width: 100%;
  border-radius: var(--border-radius-md);
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  overflow: hidden;
  cursor: default;
  transition: border-color var(--transition), background var(--transition);
  contain: layout style;
  position: relative;
}
.card:hover    { background: var(--bg-card-hover); }
.card:focus    { outline: none; }
.card.selected { border-color: var(--color-accent); background: var(--bg-card-selected); }

.thumb-wrap {
  position: relative;
  width: 100%;
  height: 150px;
  background: #111;
  overflow: hidden;
}
.thumb { width: 100%; height: 100%; object-fit: cover; display: block; }

.thumb-placeholder {
  width: 100%; height: 100%;
  display: flex; flex-direction: column;
  align-items: center; justify-content: center;
  gap: 4px; background: #1a1a1a;
}
.thumb-ext {
  font-size: 11px; font-weight: 700; color: var(--text-muted);
  background: var(--bg-card); padding: 2px 6px;
  border-radius: var(--border-radius-sm);
  border: 1px solid var(--border-color);
}
.thumb-no-preview { font-size: 9px; color: var(--text-muted); opacity: 0.6; }

.thumb-loading {
  width: 20px; height: 20px;
  border: 2px solid #333;
  border-top-color: var(--color-accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

.selected-overlay {
  position: absolute; inset: 0;
  background: rgba(74,144,217,0.35);
  display: flex; align-items: center; justify-content: center;
}
.checkmark { font-size: 28px; color: #fff; text-shadow: 0 1px 4px rgba(0,0,0,0.5); }

.meta {
  padding: var(--space-2);
  display: flex; flex-direction: column; gap: 2px;
}
.meta-name {
  font-size: var(--font-size-xs); color: var(--text-primary);
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  font-weight: 500;
}
.meta-detail { font-size: 10px; color: var(--text-muted); }

.meta-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 4px;
}

.btn-group {
  display: flex;
  gap: 3px;
}

.btn-open {
  padding: 2px 6px;
  font-size: 10px; font-weight: 500;
  background: var(--bg-secondary);
  color: var(--text-secondary);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  cursor: pointer;
  transition: background var(--transition), color var(--transition), border-color var(--transition);
}
.btn-open:hover    { background: var(--color-accent);  color: #fff; border-color: var(--color-accent);  }
.btn-explore:hover { background: var(--color-success); color: #fff; border-color: var(--color-success); }

.card-checkbox {
  display: flex;
  align-items: center;
  cursor: pointer;
}
.card-checkbox input[type="checkbox"] {
  width: 14px; height: 14px;
  accent-color: var(--color-accent);
  cursor: pointer;
  opacity: 0.5;
  transition: opacity var(--transition);
}
.card:hover .card-checkbox input[type="checkbox"],
.card-checkbox input[type="checkbox"]:checked {
  opacity: 1;
}

.btn-edit-exif {
  padding: 5px 12px;
  border-radius: var(--border-radius-sm);
  font-size: var(--font-size-sm);
  font-weight: 600;
  border: 1px solid var(--color-accent);
  background: var(--color-accent);
  color: #fff;
  transition: opacity var(--transition);
}
.btn-edit-exif:hover:not(:disabled) { opacity: 0.85; }
.btn-edit-exif:disabled { opacity: 0.35; cursor: not-allowed; }

.meta-manager-root {
  flex: 1;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  position: relative;
}
</style>
