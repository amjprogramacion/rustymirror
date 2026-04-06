<template>
  <aside class="sidebar" :style="{ width: sidebarWidth + 'px', minWidth: sidebarWidth + 'px' }">

    <!-- Branding -->
    <div class="sidebar-header">
      <div class="header-brand">
        <span class="app-name">RustyMirror</span>
        <span class="app-version">v{{ version }}<span v-if="devSuffix" class="app-version-dev">{{ devSuffix }}</span></span>
      </div>
      <button class="btn-settings" @click="showSettings = true" title="Settings">
        <svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="3"/>
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>
        </svg>
        <span v-if="updateStatus === 'available'" class="update-badge" />
      </button>
    </div>

    <SettingsModal v-model="showSettings" />

    <div class="sidebar-divider" />

    <!-- Folder list -->
    <section class="sidebar-section">
      <p class="section-label">Folders</p>

      <ul class="folder-list" v-if="store.folders.length">
        <li v-for="folder in store.folders" :key="folder" class="folder-item">
          <span class="folder-path" :title="folder">{{ folder }}</span>
          <button class="btn-remove" @click="store.removeFolder(folder)" title="Remove folder">✕</button>
        </li>
      </ul>
      <p class="empty-hint" v-else>No folders added yet</p>

      <button class="btn btn-secondary btn-full" @click="pickFolder">
        + Add folder
      </button>
    </section>

    <div class="sidebar-divider" />

    <!-- Similarity threshold -->
    <section class="sidebar-section">
      <div class="threshold-header">
        <p class="section-label">Similarity</p>
        <span class="threshold-value">{{ store.similarityThreshold }}%</span>
      </div>
      <input
        type="range"
        min="75"
        max="100"
        step="1"
        v-model.number="store.similarityThreshold"
        class="threshold-slider"
        :disabled="store.scanning"
      />
      <p class="threshold-hint">
        <span v-if="store.similarityThreshold === 100">Exact matches only</span>
        <span v-else-if="store.similarityThreshold >= 95">Very similar ({{ store.similarityThreshold }}%+)</span>
        <span v-else-if="store.similarityThreshold >= 85">Similar ({{ store.similarityThreshold }}%+)</span>
        <span v-else>Loosely similar ({{ store.similarityThreshold }}%+)</span>
      </p>
    </section>

    <div class="sidebar-divider" />

    <!-- Scan button -->
    <section class="sidebar-section">
      <button
        class="btn btn-full"
        :class="store.scanning ? 'btn-danger' : 'btn-success'"
        :disabled="!store.scanning && store.folders.length === 0"
        @click="store.scanning ? store.stopScan() : store.startScan()"
      >
        {{ store.scanning ? 'Stop scan' : 'Scan' }}
      </button>

    </section>

    <!-- Filter pills — only after scan -->
    <template v-if="store.scanDone">
      <div class="sidebar-divider" />
      <section class="sidebar-section">
        <p class="section-label">Filter</p>
        <div class="filter-pills">
          <button
            v-for="f in filters"
            :key="f.value"
            class="pill"
            :class="{ active: store.filter === f.value }"
            @click="store.filter = f.value"
          >
            {{ f.label }}
            <span class="pill-count">{{ store.groupCounts[f.value] }}</span>
          </button>
        </div>
      </section>
    </template>

    <!-- Recent scans -->
    <template v-if="history.entries.length > 0">
      <div class="sidebar-divider" />
      <section class="sidebar-section">
        <p class="section-label">Recent scans</p>
        <div
          v-for="entry in history.entries"
          :key="entry.id"
          class="history-entry"
          :class="{
            disabled: store.scanning,
            active: isActiveEntry(entry),
          }"
          @click="loadFromHistory(entry)"
          :title="entry.folders.join('\n')"
        >
          <!-- Line 1: date & time (+ scan duration on first real scan) -->
          <div class="history-date">
            {{ formatLocalDate(entry.date) }}<span v-if="formatDuration(entry.durationMs)" class="history-duration">&nbsp;({{ formatDuration(entry.durationMs) }})</span>
          </div>

          <!-- Line 2: folder path(s) -->
          <div class="history-folders">
            <span v-for="f in entry.folders" :key="f" class="history-folder" :title="f">
              {{ shortPath(f) }}
            </span>
          </div>

          <!-- Line 3: stats left · threshold badge right -->
          <div class="history-footer">
            <span class="history-stats">
              {{ entry.imageCount ?? 0 }} img · {{ entry.duplicates }} group{{ entry.duplicates !== 1 ? 's' : '' }}
            </span>
            <div class="history-badges">
              <span v-if="entry.fastMode" class="history-fast-badge" title="Fast mode (EXIF thumbnail)">
                <svg viewBox="0 0 7 11" fill="none" xmlns="http://www.w3.org/2000/svg">
                  <path d="M4.2 0L0 6h3.2L2.8 11 7 5H3.8L4.2 0z" fill="#f5c542"/>
                </svg>
              </span>
              <span class="history-threshold">{{ entry.threshold ?? 90 }}%</span>
            </div>
          </div>
        </div>
      </section>
    </template>

    <!-- Cache buttons — pinned to bottom -->
    <div class="sidebar-spacer" />
    <div class="sidebar-divider" />
    <section class="sidebar-section sidebar-bottom">
      <button
        class="btn btn-cache btn-full btn-sm"
        :class="{ 'btn-cache--active': history.entries.length > 0 }"
        @click="history.clearHistory()"
        :disabled="history.entries.length === 0"
      >
        Clear scan history
      </button>
      <button
        class="btn btn-cache btn-full btn-sm"
        :class="{ 'btn-cache--active': cacheSize > 0 }"
        @click="clearCache"
        :disabled="cacheSize === 0"
        :title="`Hash cache: ${formatSize(cacheSize)}`"
      >
        Clear hash cache
        <span class="cache-size" v-if="cacheSize > 0">{{ formatSize(cacheSize) }}</span>
      </button>
      <button
        class="btn btn-cache btn-full btn-sm"
        :class="{ 'btn-cache--active': thumbCacheSize > 0 }"
        @click="clearThumbCache"
        :disabled="thumbCacheSize === 0"
        :title="`Thumbnail cache: ${formatSize(thumbCacheSize)}`"
      >
        Clear thumbnail cache
        <span class="cache-size" v-if="thumbCacheSize > 0">{{ formatSize(thumbCacheSize) }}</span>
      </button>
    </section>

    <!-- Resize handle -->
    <div class="sidebar-resizer" @mousedown.prevent="startResize" />

  </aside>
</template>

<script setup>
import { ref, onMounted, onBeforeUnmount, watch } from 'vue'
import { useSettings } from '../composables/useSettings'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { useScanStore } from '../store/scan'
import { useHistoryStore } from '../store/history'
import { formatSize, shortPath, formatLocalDate, formatDuration } from '../utils/formatters'
import { useCacheSize } from '../composables/useCacheSize'
import { useUpdater } from '../composables/useUpdater'
import { SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH } from '../constants'
import SettingsModal from './SettingsModal.vue'

const store = useScanStore()
const history = useHistoryStore()
const { status: updateStatus } = useUpdater()
const baseVersion = import.meta.env.VITE_APP_VERSION ?? '0.1.0'
const isDev = import.meta.env.DEV
const version = ref(baseVersion)
const devSuffix = ref('')

onMounted(async () => {
  if (isDev) {
    const isDebug = await invoke('is_debug_build')
    devSuffix.value = isDebug ? '.dev' : '.dev-release'
  }
})
const showSettings = ref(false)

const filters = [
  { label: 'All',     value: 'all'      },
  { label: 'Exact',   value: 'exact'    },
  { label: 'Similar', value: 'similar'  },
  { label: 'Dates',   value: 'sameDate' },
]

// Scroll results to top whenever the active filter changes
watch(() => store.filter, () => {
  document.getElementById('groups-scroll')?.scrollTo({ top: 0, behavior: 'smooth' })
})

// ── Cache management ──────────────────────────────────────────────────────────
const { cacheSize, thumbCacheSize, loadCacheSizes: loadCacheSize, clearCache, clearThumbCache } = useCacheSize()


function isActiveEntry(entry) {
  return store.scanDone && entry.id === store.activeHistoryEntryId
}

function loadFromHistory(entry) {
  if (store.scanning) return

  const entryThreshold = entry.threshold ?? 90

  store.folders = [...entry.folders]
  store.similarityThreshold = entryThreshold

  // Already showing this exact entry — nothing to do
  if (entry.id === store.activeHistoryEntryId) return

  // Restore the fastMode that was used when this entry was originally scanned,
  // regardless of what the current Settings toggle says.
  store.fastModeOverride = entry.fastMode ?? false
  store.scanLabel = 'Loading scan…'
  store.startScan()
}


onMounted(() => {
  loadCacheSize()
  setTimeout(loadCacheSize, 1500)
})

watch(() => store.scanDone, (done) => { if (done) loadCacheSize() })
watch(() => store.scanning, (scanning) => { if (!scanning) loadCacheSize() })
watch(() => store.heicThumbGenerated, () => { loadCacheSize() })

async function pickFolder() {
  const path = await open({ directory: true, multiple: false })
  if (path) store.addFolder(path)
}

// ── Sidebar resize ────────────────────────────────────────────────────────────

const { sidebarWidth } = useSettings()

let resizing = false
let startX = 0
let startWidth = 0

function startResize(e) {
  resizing = true
  startX = e.clientX
  startWidth = sidebarWidth.value
  document.body.style.cursor = 'col-resize'
  document.body.style.userSelect = 'none'
}

function onMouseMove(e) {
  if (!resizing) return
  const delta = e.clientX - startX
  sidebarWidth.value = Math.min(SIDEBAR_MAX_WIDTH, Math.max(SIDEBAR_MIN_WIDTH, startWidth + delta))
}

function onMouseUp() {
  if (!resizing) return
  resizing = false
  document.body.style.cursor = ''
  document.body.style.userSelect = ''
}

onMounted(() => {
  document.addEventListener('mousemove', onMouseMove)
  document.addEventListener('mouseup', onMouseUp)
})

onBeforeUnmount(() => {
  document.removeEventListener('mousemove', onMouseMove)
  document.removeEventListener('mouseup', onMouseUp)
})
</script>

<style scoped>
.sidebar {
  position: relative;
  background: var(--sidebar-bg);
  border-right: 1px solid var(--sidebar-border);
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow-y: auto;
  overflow-x: hidden;
}

/* ── Resize handle ── */
.sidebar-resizer {
  position: absolute;
  top: 0;
  right: -3px;
  width: 6px;
  height: 100%;
  cursor: col-resize;
  z-index: 10;
}
.sidebar-resizer:hover,
.sidebar-resizer:active {
  background: var(--color-accent);
  opacity: 0.4;
}

/* ── Branding ── */
.sidebar-header {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: var(--space-4) var(--space-3);
  min-height: 72px;
}

.header-brand {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
}

.app-name {
  font-size: var(--font-size-xl);
  font-weight: 600;
  color: var(--text-primary);
  letter-spacing: -0.3px;
}

.app-version {
  font-size: var(--font-size-xs);
  color: var(--color-accent);
  font-weight: 600;
  letter-spacing: 0.6px;
}

.btn-settings {
  position: absolute;
  top: var(--space-2);
  right: var(--space-2);
  background: none;
  color: var(--text-muted);
  padding: 5px;
  border-radius: var(--border-radius-sm);
  display: flex;
  align-items: center;
  justify-content: center;
  transition: color var(--transition), background var(--transition);
}
.btn-settings:hover {
  color: var(--text-secondary);
  background: var(--bg-card);
}

.update-badge {
  position: absolute;
  top: 2px;
  right: 2px;
  width: 7px;
  height: 7px;
  background: var(--color-accent);
  border-radius: 50%;
  border: 1.5px solid var(--sidebar-bg);
  pointer-events: none;
}

/* ── Divider ── */
.sidebar-divider {
  height: 1px;
  background: var(--sidebar-border);
  margin: 0;
}

/* ── Section ── */
.sidebar-section {
  padding: var(--space-3);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.section-label {
  font-size: var(--font-size-xs);
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.6px;
}

/* ── Folder list ── */
.folder-list {
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.folder-item {
  display: flex;
  align-items: flex-start;
  gap: var(--space-1);
  padding: 4px var(--space-1);
  border-radius: var(--border-radius-sm);
  background: var(--bg-card);
}

.folder-path {
  flex: 1;
  font-size: var(--font-size-xs);
  color: var(--text-secondary);
  word-break: break-all;
  line-height: 1.4;
}

.btn-remove {
  background: none;
  color: var(--color-danger);
  font-size: 11px;
  padding: 0 2px;
  line-height: 1;
  flex-shrink: 0;
  margin-top: 1px;
  transition: color var(--transition);
}
.btn-remove:hover { color: var(--color-danger-hover); }

.empty-hint {
  font-size: var(--font-size-xs);
  color: var(--text-muted);
  font-style: italic;
}

/* ── Buttons ── */
.btn {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 7px var(--space-3);
  border-radius: var(--border-radius-md);
  font-size: var(--font-size-sm);
  font-weight: 500;
  transition: background var(--transition), color var(--transition),
              border-color var(--transition), opacity var(--transition);
}

.btn-full { width: 100%; }

.btn-secondary {
  background: var(--bg-card);
  color: var(--text-secondary);
  border: 1px solid var(--border-color);
}
.btn-secondary:hover:not(:disabled) { background: var(--bg-card-hover); }

.btn-success {
  background: var(--color-success);
  color: #fff;
}
.btn-success:hover:not(:disabled) { background: var(--color-success-hover); }

.btn-danger {
  background: var(--color-danger);
  color: #fff;
}
.btn-danger:hover:not(:disabled) { background: var(--color-danger-hover); }

/* ── Threshold slider ── */
.threshold-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.threshold-value {
  font-size: var(--font-size-sm);
  font-weight: 600;
  color: var(--color-accent);
}

.threshold-slider {
  width: 100%;
  accent-color: var(--color-accent);
  cursor: pointer;
}

.threshold-slider:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.threshold-hint {
  font-size: var(--font-size-xs);
  color: var(--text-muted);
  text-align: center;
}

/* ── History ── */
.history-entry {
  padding: var(--space-2);
  border-radius: var(--border-radius-sm);
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  cursor: pointer;
  transition: background var(--transition), border-color var(--transition);
  margin-bottom: var(--space-1);
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.history-entry:hover:not(.disabled):not(.active) {
  background: var(--bg-card-hover);
  border-color: var(--color-accent);
}
.history-entry.active {
  border-color: var(--color-accent);
  background: var(--bg-card);
}
.history-entry.disabled { opacity: 0.4; cursor: not-allowed; }

/* Line 1 — date */
.history-date {
  font-size: 10px;
  color: var(--text-muted);
}

.history-duration {
  opacity: 0.7;
}

/* Line 2 — folder path(s) */
.history-folders {
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.history-folder {
  font-size: 11px;
  color: var(--text-primary);
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* Line 3 — stats + threshold badge */
.history-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-top: 1px;
}

.history-stats {
  font-size: 10px;
  font-weight: 600;
  color: var(--color-accent);
}

.history-badges {
  display: inline-flex;
  align-items: center;
  gap: 2px;
}

.history-threshold {
  display: inline-flex;
  align-items: center;
  height: 16px;
  font-size: 9px;
  font-weight: 700;
  color: rgba(0, 0, 0, 0.65);
  background: var(--color-accent);
  border-radius: var(--border-radius-pill);
  padding: 0 6px;
  letter-spacing: 0.2px;
}

.history-fast-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  background: transparent;
  border-radius: 50%;
}

.history-fast-badge svg {
  width: 7px;
  height: 11px;
  transform: rotate(15deg);
}

/* ── Cache size badge ── */
.cache-size {
  font-size: 9px;
  opacity: 0.6;
  margin-left: 4px;
}

/* ── Bottom section ── */
.sidebar-spacer { flex: 1; }
.sidebar-bottom { padding-top: var(--space-3); padding-bottom: var(--space-3); }
.btn-sm { padding: 5px var(--space-3); font-size: 11px; }

/* ── Cache buttons ── */
.btn-cache {
  background: transparent;
  color: var(--text-muted);
  border: 1px solid transparent;
}
.btn-cache:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}
.btn-cache.btn-cache--active {
  background: var(--bg-card);
  color: var(--text-secondary);
  border-color: var(--border-color);
}
.btn-cache.btn-cache--active:hover {
  background: var(--bg-card-hover);
  color: #fff;
  border-color: var(--bg-card-hover);
}

/* ── Filter pills ── */
.filter-pills {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-1);
}

.pill {
  flex: 1 1 calc(50% - var(--space-1));
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 4px;
  padding: 2px 3px 2px 8px;
  font-size: var(--font-size-xs);
  font-weight: 500;
  text-transform: uppercase;
  background: var(--bg-card);
  color: var(--text-secondary);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-pill);
  transition: background var(--transition), color var(--transition), border-color var(--transition);
}

.pill:hover { background: var(--bg-card-hover); }

.pill.active {
  background: var(--color-accent);
  color: #fff;
  border-color: var(--color-accent);
}

.pill-count {
  opacity: 0.7;
  font-weight: 400;
  background: var(--bg-primary);
  padding: 2px 6px;
  border-radius: 1rem;
}

.pill.active .pill-count {
  background: #092a4d;
}
</style>
