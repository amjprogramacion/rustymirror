<template>
<div class="meta-manager-root">
  <!-- Scanning / geocoding progress -->
  <ScanProgress
    v-if="meta.scanning || (meta.geocoding && prefetchFilters)"
    :title="meta.geocoding ? 'Fetching locations…' : 'Scanning images…'"
    :subtitle="meta.geocoding ? `${meta.images.length.toLocaleString()} images found` : null"
  />

  <!-- Empty state -->
  <div class="empty-state" v-else-if="!meta.scanDone">
    <p>Add folders and press <strong>Scan</strong> to load images.</p>
  </div>

  <!-- Results -->
  <template v-else>
    <FailedFilesWarning :files="meta.failedFiles" />

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
      <button
        class="btn btn-danger"
        :disabled="meta.selectedCount === 0"
        @click="confirmDelete"
      >
        Delete selected
        <span class="badge" v-if="meta.selectedCount > 0">{{ meta.selectedCount }}</span>
      </button>

      <span class="image-count">{{ meta.filteredImages.length }} image{{ meta.filteredImages.length !== 1 ? 's' : '' }}<template v-if="meta.selectedCount > 0"> · {{ meta.selectedCount }} selected</template></span>

      <!-- Search -->
      <SearchInput v-model="meta.searchQuery" />
    </div>

    <!-- Grid -->
    <div class="grid-scroll" ref="gridEl" :style="panel.activePanel ? { paddingBottom: (panel.panelHeight + 16) + 'px' } : {}">
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
          :class="{ selected: meta.selected.has(entry.path), focused: !panel.activePanel?.batch && panel.activePanel?.entry?.path === entry.path && !meta.selected.has(entry.path) }"
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

  <BatchEditPanel />

  <!-- Confirm delete dialog -->
  <Transition name="overlay-fade">
    <div class="overlay" v-if="showConfirm" @click.self="closeConfirm">
      <div class="dialog" :class="{ 'dialog--thumbs': viewMode === 'thumbs' }">

        <!-- Header row: icon + title + view toggle -->
        <div class="dialog-header">
          <div class="dialog-icon">🗑️</div>
          <h3 class="dialog-title">Delete {{ meta.selectedCount }} image{{ meta.selectedCount !== 1 ? 's' : '' }}?</h3>
          <button
            class="view-toggle"
            :title="viewMode === 'list' ? 'Show thumbnails' : 'Show list'"
            @click="toggleViewMode"
          >
            <!-- grid icon -->
            <svg v-if="viewMode === 'list'" width="15" height="15" viewBox="0 0 15 15" fill="none">
              <rect x="1" y="1" width="5.5" height="5.5" rx="1" fill="currentColor"/>
              <rect x="8.5" y="1" width="5.5" height="5.5" rx="1" fill="currentColor"/>
              <rect x="1" y="8.5" width="5.5" height="5.5" rx="1" fill="currentColor"/>
              <rect x="8.5" y="8.5" width="5.5" height="5.5" rx="1" fill="currentColor"/>
            </svg>
            <!-- list icon -->
            <svg v-else width="15" height="15" viewBox="0 0 15 15" fill="none">
              <rect x="1" y="2.5" width="13" height="1.5" rx="0.75" fill="currentColor"/>
              <rect x="1" y="6.75" width="13" height="1.5" rx="0.75" fill="currentColor"/>
              <rect x="1" y="11" width="13" height="1.5" rx="0.75" fill="currentColor"/>
            </svg>
          </button>
        </div>

        <!-- List mode: all files, scrollable -->
        <div v-if="viewMode === 'list'" class="dialog-files dialog-files--list">
          <div
            v-for="path in allPaths"
            :key="path"
            class="dialog-file"
            :title="path"
          >
            <span
              v-if="networkRoots.size > 0"
              class="file-dot"
              :class="pathIsNetwork(path) ? 'dot-network' : 'dot-local'"
              :title="pathIsNetwork(path) ? 'Network drive' : 'Local drive'"
            />
            {{ fileName(path) }}
          </div>
        </div>

        <!-- Thumbnail mode: grid -->
        <div v-else class="dialog-files dialog-files--thumbs">
          <div
            v-for="path in allPaths"
            :key="path"
            class="dialog-thumb"
            :title="path"
          >
            <img
              v-if="thumbSrc(path)"
              :src="thumbSrc(path)"
              class="thumb-img"
              loading="lazy"
            />
            <div v-else class="thumb-loading-spin">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <circle cx="12" cy="12" r="10" stroke-opacity="0.25"/>
                <path d="M12 2a10 10 0 0 1 10 10" stroke-linecap="round"/>
              </svg>
            </div>
            <span
              v-if="networkRoots.size > 0"
              class="thumb-badge"
              :class="pathIsNetwork(path) ? 'dot-network' : 'dot-local'"
            />
          </div>
        </div>

        <!-- Legend when mixed -->
        <div v-if="hasMixed" class="dialog-legend">
          <span class="file-dot dot-local" /> Local &nbsp;
          <span class="file-dot dot-network" /> Network
        </div>

        <p class="dialog-warning" :class="{ 'dialog-warning--danger': hasNetworkFiles }">
          <template v-if="hasMixed">
            ⚠️ Local files will go to the <strong>system trash</strong>. Network files will be <strong>permanently deleted</strong>.
          </template>
          <template v-else-if="hasNetworkFiles">
            ⛔ These files are on a <strong>network drive</strong>. They will be <strong>permanently deleted</strong> and cannot be recovered.
          </template>
          <template v-else>
            ⚠️ Files will be moved to the <strong>system trash</strong>. You can restore them from there if needed.
          </template>
        </p>

        <p v-if="deleteError" class="dialog-error">{{ deleteError }}</p>

        <div class="dialog-actions">
          <button class="btn btn-ghost" @click="closeConfirm">Cancel</button>
          <button class="btn btn-danger" @click="doDelete" :disabled="deleting">
            <span v-if="deleting && deleteProgress.total > 0">
              Deleting {{ deleteProgress.done }}/{{ deleteProgress.total }}…
            </span>
            <span v-else-if="deleting">Deleting…</span>
            <span v-else>Delete {{ meta.selectedCount }} file{{ meta.selectedCount !== 1 ? 's' : '' }}</span>
          </button>
        </div>
      </div>
    </div>
  </Transition>
</div>
</template>

<script setup>
import { ref, computed, onBeforeUnmount, watch, nextTick } from 'vue'
import { convertFileSrc, invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useMetadataStore } from '../store/metadata'
import { usePanelStore } from '../store/panel'
import { useThumbnailStore } from '../store/thumbnails'
import { useSettings } from '../composables/useSettings'
import { fileExt, fileName, formatSize, formatDate } from '../utils/formatters'
import { errorMessage } from '../utils/errors'
import BatchEditPanel from './BatchEditPanel.vue'
import SearchInput from './SearchInput.vue'
import ScanProgress from './ScanProgress.vue'
import FailedFilesWarning from './FailedFilesWarning.vue'

const meta   = useMetadataStore()
const panel  = usePanelStore()
const thumbs = useThumbnailStore()
const { prefetchFilters } = useSettings()
const gridEl    = ref(null)
const THUMB_ERROR = '__error__'
const HEIC_EXTS   = new Set(['heic', 'heif'])

const thumbCache     = thumbs.thumbCache
const directSrcCache = thumbs.directSrcCache

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
          thumbs.enqueueThumbnail(path)
        } else {
          thumbs.setDirectSrc(path, convertFileSrc(path))
          observer.unobserve(e.target)
        }
      } else {
        thumbs.dequeueThumbnail(path)
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
    const active = panel.activePanel
    if (active && !active.batch && active.entry?.path) {
      meta.toggleSelected(active.entry.path)
    }
  }
}

function openBatchEdit() {
  const selected = meta.filteredImages.filter(e => meta.selected.has(e.path))
  if (selected.length < 2) return
  panel.openBatchPanel(selected)
}

// Reactive key that changes whenever the selected set changes (add, remove, swap)
const selectedKey = computed(() => {
  const arr = []
  meta.selected.forEach(p => arr.push(p))
  return arr.sort().join('|')
})

// Keep the panel in sync with selection changes
watch(selectedKey, () => {
  const active = panel.activePanel
  if (!active || active.saving) return

  const count = meta.selectedCount
  if (count === 0) {
    panel.closePanel()
  } else if (count === 1) {
    const entry = meta.filteredImages.find(e => meta.selected.has(e.path))
    if (entry) panel.openPanel(entry)
  } else {
    const entries = meta.filteredImages.filter(e => meta.selected.has(e.path))
    panel.openBatchPanel(entries)
  }
})

// Close the metadata panel on sort, filter, or new scan
watch(() => [meta.sortBy, meta.sortDir], () => panel.closePanel())
watch(() => [meta.filterDateFrom, meta.filterDateTo, meta.filterLocation, meta.filterDevice], () => panel.closePanel())
watch(() => meta.scanning, (scanning) => { if (scanning) panel.closePanel() })

// When visible images change (filter/sort), cancel pending thumb loads and
// re-register the observer so newly visible cards get prioritised.
watch(
  () => meta.filteredImages,
  () => {
    thumbs.clearThumbQueue()
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
function onCardClick(entry) {
  if (meta.multiSelect) {
    meta.toggleSelected(entry.path)
  } else {
    if (panel.activePanel?.entry?.path === entry.path) return
    panel.openPanel(entry)
  }
}

// ── File actions ──────────────────────────────────────────────────────────────
async function openFile(path)   { await invoke('open_file',   { path }) }
async function openFolder(path) { await invoke('open_folder', { path }) }

// ── Delete dialog ─────────────────────────────────────────────────────────────
const showConfirm    = ref(false)
const deleteError    = ref(null)
const viewMode       = ref('list') // 'list' | 'thumbs'
const deleting       = ref(false)
const deleteProgress = ref({ done: 0, total: 0 })
const networkRoots   = ref(new Set())

const allPaths = computed(() => [...meta.selected])

function pathIsNetwork(p) {
  for (const root of networkRoots.value) {
    if (p.startsWith(root)) return true
  }
  return false
}

const hasNetworkFiles = computed(() => allPaths.value.some(pathIsNetwork))
const hasMixed = computed(() =>
  hasNetworkFiles.value && allPaths.value.some(p => !pathIsNetwork(p))
)

function thumbSrc(path) {
  const cached = thumbs.thumbCache[path]
  if (cached && cached !== '__error__') return cached
  const direct = thumbs.directSrcCache[path]
  if (direct) return direct
  return null
}

function toggleViewMode() {
  viewMode.value = viewMode.value === 'list' ? 'thumbs' : 'list'
  if (viewMode.value === 'thumbs') {
    for (const path of meta.selected) thumbs.enqueueThumbnail(path)
  }
}

function closeConfirm() {
  showConfirm.value = false
  deleteError.value = null
  viewMode.value = 'list'
}

async function confirmDelete() {
  if (meta.selectedCount === 0) return
  try {
    const checks = await Promise.all(
      meta.folders.map(f => invoke('is_network_path', { path: f }))
    )
    networkRoots.value = new Set(meta.folders.filter((_, i) => checks[i]))
  } catch {
    networkRoots.value = new Set()
  }
  showConfirm.value = true
}

async function doDelete() {
  deleting.value = true
  deleteError.value = null
  deleteProgress.value = { done: 0, total: meta.selectedCount }

  const unlisten = await listen('delete_progress', (e) => {
    deleteProgress.value = e.payload
  })

  try {
    await meta.deleteSelected()
    closeConfirm()
    meta.multiSelect = false
  } catch (e) {
    deleteError.value = `Delete failed: ${errorMessage(e)}`
  } finally {
    unlisten()
    deleting.value = false
    deleteProgress.value = { done: 0, total: 0 }
  }
}
</script>

<style scoped>
/* ── States ── */
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

/* ── Grid scroll ── */
.grid-scroll {
  flex: 1;
  overflow-y: auto;
  padding: var(--space-4);
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
.card::after {
  content: '';
  position: absolute;
  inset: 0;
  border-radius: var(--border-radius-md);
  pointer-events: none;
  border: 2px solid transparent;
  box-shadow: none;
  transition: border-color var(--transition), box-shadow var(--transition);
  z-index: 10;
}
.card.focused::after {
  border-color: #7ab8f5;
  box-shadow: inset 0 0 18px 4px rgba(122, 184, 245, 0.3);
}

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
  border-radius: var(--border-radius-md);
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

/* ── Delete button & badge ── */
.btn-danger { background: var(--color-danger); color: #fff; border: none; }
.btn-danger:hover:not(:disabled) { background: var(--color-danger-hover); }

.badge {
  background: rgba(255,255,255,0.25);
  border-radius: var(--border-radius-pill);
  padding: 1px 6px;
  font-size: 11px;
  font-weight: 600;
}

/* ── Overlay & dialog ── */
.overlay {
  position: fixed;
  inset: 0;
  background: rgba(0,0,0,0.65);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
}

.overlay-fade-enter-active, .overlay-fade-leave-active { transition: opacity 150ms ease; }
.overlay-fade-enter-from, .overlay-fade-leave-to { opacity: 0; }

.dialog {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-lg);
  padding: var(--space-5);
  width: 420px;
  max-width: calc(100vw - 40px);
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  box-shadow: 0 20px 60px rgba(0,0,0,0.5);
  transition: width 220ms ease;
}
.dialog--thumbs { width: 640px; }

.dialog-header {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}
.dialog-icon { font-size: 22px; flex-shrink: 0; }
.dialog-title {
  flex: 1;
  font-size: var(--font-size-lg);
  font-weight: 600;
  color: var(--text-primary);
  margin: 0;
}
.view-toggle {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: var(--border-radius-sm);
  border: 1px solid var(--border-color);
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  transition: background var(--transition), color var(--transition);
}
.view-toggle:hover { background: var(--bg-secondary); color: var(--text-primary); }

.dialog-files {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  overflow-y: auto;
  transition: max-height 220ms ease;
}

.dialog-files--list {
  max-height: 220px;
  padding: var(--space-2);
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.dialog-file {
  display: flex;
  align-items: center;
  font-size: var(--font-size-sm);
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  padding: 5px var(--space-2);
  border-radius: 3px;
  min-height: 28px;
}
.dialog-file:nth-child(odd) { background: rgba(255,255,255,0.03); }

.dialog-files--thumbs {
  max-height: 420px;
  padding: var(--space-2);
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(72px, 1fr));
  gap: 4px;
}
.dialog-thumb {
  position: relative;
  aspect-ratio: 1;
  border-radius: 4px;
  overflow: hidden;
  background: var(--bg-tertiary, rgba(255,255,255,0.05));
}
.thumb-img { width: 100%; height: 100%; object-fit: cover; display: block; }
.thumb-loading-spin {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-muted);
  animation: spin 1s linear infinite;
}
.thumb-badge {
  position: absolute;
  top: 3px; right: 3px;
  width: 8px; height: 8px;
  border-radius: 50%;
  border: 1px solid rgba(0,0,0,0.4);
}

.file-dot {
  display: inline-block;
  width: 7px; height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
  margin-right: 5px;
  vertical-align: middle;
}
.dot-local   { background: var(--color-success); }
.dot-network { background: var(--color-danger); }

.dialog-legend {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 10px;
  color: var(--text-muted);
  justify-content: center;
}

.dialog-warning {
  font-size: var(--font-size-xs);
  color: var(--text-muted);
  text-align: center;
  line-height: 1.5;
}
.dialog-warning--danger {
  color: var(--color-danger) !important;
  background: rgba(220, 53, 69, 0.08);
  padding: var(--space-2);
  border-radius: var(--border-radius-sm);
  border: 1px solid rgba(220, 53, 69, 0.25);
}

.dialog-error {
  font-size: var(--font-size-xs);
  color: var(--color-danger);
  text-align: center;
}

.dialog-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2);
  margin-top: var(--space-1);
}
</style>
