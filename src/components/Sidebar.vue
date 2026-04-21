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

    <!-- Mode sub-panels — each owns its own stores, logic and styles -->
    <SidebarDuplicates v-if="activeMode === 'duplicates'" />
    <SidebarMeta v-else-if="activeMode === 'metadata'" />
    <SidebarOrganizer v-else />

    <!-- Cache buttons — pinned to bottom -->
    <div class="sidebar-spacer" v-if="!((activeMode === 'duplicates' && history.entries.length > 0) || (activeMode === 'metadata' && metaHistory.entries.length > 0) || (activeMode === 'organizer' && orgHistory.entries.length > 0))" />
    <div class="sidebar-divider" />
    <section class="sidebar-section sidebar-bottom">
      <button
        v-if="activeMode === 'duplicates'"
        class="btn btn-cache btn-full btn-sm"
        :class="{ 'btn-cache--active': history.entries.length > 0 }"
        @click="history.clearHistory()"
        :disabled="history.entries.length === 0"
      >
        Clear scan history
      </button>
      <button
        v-if="activeMode === 'metadata'"
        class="btn btn-cache btn-full btn-sm"
        :class="{ 'btn-cache--active': metaHistory.entries.length > 0 }"
        @click="metaHistory.clearHistory()"
        :disabled="metaHistory.entries.length === 0"
      >
        Clear scan history
      </button>
      <button
        v-if="activeMode === 'organizer'"
        class="btn btn-cache btn-full btn-sm"
        :class="{ 'btn-cache--active': orgHistory.entries.length > 0 }"
        @click="orgHistory.clearHistory()"
        :disabled="orgHistory.entries.length === 0"
      >
        Clear scan history
      </button>
      <button
        v-if="activeMode !== 'organizer'"
        class="btn btn-cache btn-full btn-sm"
        :class="{ 'btn-cache--active': thumbCacheSize > 0 }"
        @click="clearThumbCache"
        :disabled="thumbCacheSize === 0"
        :title="`Thumbnail cache: ${formatSize(thumbCacheSize)}`"
      >
        Clear thumbnail cache
        <span class="cache-size" v-if="thumbCacheSize > 0">{{ formatSize(thumbCacheSize) }}</span>
      </button>
      <button
        v-if="activeMode === 'duplicates'"
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
        v-if="activeMode === 'metadata'"
        class="btn btn-cache btn-full btn-sm"
        :class="{ 'btn-cache--active': meta.geoCacheCount > 0 }"
        @click="meta.clearGeoCache()"
        :disabled="meta.geoCacheCount === 0"
        :title="`Location cache: ${meta.geoCacheCount} entr${meta.geoCacheCount === 1 ? 'y' : 'ies'} · ${formatSize(meta.geoCacheBytes)}`"
      >
        Clear location cache
        <span class="cache-size" v-if="meta.geoCacheCount > 0">{{ formatSize(meta.geoCacheBytes) }}</span>
      </button>
    </section>

    <!-- Resize handle -->
    <div class="sidebar-resizer" @mousedown.prevent="startResize" />

  </aside>
</template>

<script setup>
import { ref, onMounted, onBeforeUnmount, watch } from 'vue'
import { useMode } from '../composables/useMode'
import { useSettings } from '../composables/useSettings'
import { invoke } from '@tauri-apps/api/core'
import { useDuplicatesStore } from '../store/duplicates'
import { useMetadataStore } from '../store/metadata'
import { useThumbnailStore } from '../store/thumbnails'
import { useDuplicatesHistoryStore } from '../store/duplicatesHistory'
import { useMetadataHistoryStore } from '../store/metadataHistory'
import { useOrganizerHistoryStore } from '../store/organizerHistory'
import { formatSize } from '../utils/formatters'
import { useCacheSize } from '../composables/useCacheSize'
import { useUpdater } from '../composables/useUpdater'
import { SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH } from '../constants'
import SettingsModal from './SettingsModal.vue'
import SidebarDuplicates from './SidebarDuplicates.vue'
import SidebarMeta from './SidebarMeta.vue'
import SidebarOrganizer from './SidebarOrganizer.vue'

const { activeMode } = useMode()
const store       = useDuplicatesStore()
const meta        = useMetadataStore()
const thumbStore  = useThumbnailStore()
const history     = useDuplicatesHistoryStore()
const metaHistory = useMetadataHistoryStore()
const orgHistory  = useOrganizerHistoryStore()

const { status: updateStatus } = useUpdater()
const baseVersion = import.meta.env.VITE_APP_VERSION ?? '0.1.0'
const isDev       = import.meta.env.DEV
const version     = ref(baseVersion)
const devSuffix   = ref('')

onMounted(async () => {
  if (isDev) {
    const isDebug = await invoke('is_debug_build')
    devSuffix.value = isDebug ? '.dev' : '.dev-release'
  }
  meta.loadGeoCacheCount()
})

const showSettings = ref(false)

// ── Cache management ──────────────────────────────────────────────────────────
const { cacheSize, thumbCacheSize, loadCacheSizes: loadCacheSize, clearCache, clearThumbCache } = useCacheSize()

onMounted(() => {
  loadCacheSize()
  setTimeout(loadCacheSize, 1500)
})

// Duplicate scan
watch(() => store.scanDone,  (done)     => { if (done)      loadCacheSize() })
watch(() => store.scanning,  (scanning) => { if (!scanning) loadCacheSize() })
// Metadata scan
watch(() => meta.scanDone,   (done)     => { if (done)      loadCacheSize() })
watch(() => meta.scanning,   (scanning) => { if (!scanning) loadCacheSize() })
// Thumbnails — debounced to avoid one invoke per thumbnail when browsing
let _thumbDebounce = null
watch(() => thumbStore.heicThumbGenerated, () => {
  clearTimeout(_thumbDebounce)
  _thumbDebounce = setTimeout(loadCacheSize, 400)
})

// ── Sidebar resize ────────────────────────────────────────────────────────────
const { sidebarWidth } = useSettings()

let resizing  = false
let startX    = 0
let startWidth = 0

function startResize(e) {
  resizing = true
  startX   = e.clientX
  startWidth = sidebarWidth.value
  document.body.style.cursor     = 'col-resize'
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
  document.body.style.cursor     = ''
  document.body.style.userSelect = ''
}

onMounted(() => {
  document.addEventListener('mousemove', onMouseMove)
  document.addEventListener('mouseup',  onMouseUp)
})

onBeforeUnmount(() => {
  document.removeEventListener('mousemove', onMouseMove)
  document.removeEventListener('mouseup',  onMouseUp)
})
</script>

<style scoped>
/* ── Sidebar shell ── */
.sidebar {
  position: relative;
  background: var(--sidebar-bg);
  border-right: 1px solid var(--sidebar-border);
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
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

/* ── Divider (cache section) ── */
.sidebar-divider {
  height: 1px;
  background: var(--sidebar-border);
  margin: 0;
}

/* ── Cache section ── */
.sidebar-spacer { flex: 1; }

.sidebar-section {
  padding: var(--space-3);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.sidebar-bottom { padding-top: var(--space-3); padding-bottom: var(--space-3); }

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
.btn-sm   { padding: 5px var(--space-3); font-size: 11px; }

.btn-cache {
  background: transparent;
  color: var(--text-muted);
  border: 1px solid transparent;
}
.btn-cache:disabled { opacity: 0.3; cursor: not-allowed; }
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

.cache-size {
  font-size: 9px;
  opacity: 0.6;
  margin-left: 4px;
}
</style>
