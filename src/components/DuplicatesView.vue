<template>
  <!-- Scanning progress -->
  <ScanProgress
    v-if="store.scanning"
    :fingerprinting="store.fingerprinting"
    :scan-label="store.scanLabel"
    :progress="store.progress"
    :progress-percent="store.progressPercent"
    :analyze-progress="store.analyzeProgress"
    :eta-seconds="store.etaSeconds"
  />

  <!-- Empty state -->
  <div class="empty-state" v-else-if="!store.scanDone">
    <p>Add folders and press <strong>Scan</strong> to find duplicate images.</p>
  </div>

  <!-- Results -->
  <template v-else>
    <FailedFilesWarning :files="store.failedFiles" />

    <!-- Action bar -->
    <div class="action-bar">
      <button class="btn btn-ghost" @click="store.selectCopies()">Select copies</button>
      <button
        class="btn btn-ghost"
        :class="{ active: store.multiSelect }"
        @click="store.multiSelect = !store.multiSelect"
      >Multi-select</button>
      <button
        class="btn btn-ghost"
        :disabled="store.selectedCount === 0"
        @click="store.clearSelection(); store.multiSelect = false"
      >Deselect all</button>
      <button
        class="btn btn-danger"
        :disabled="store.selectedCount === 0"
        @click="confirmDelete"
      >
        Delete selected
        <span class="badge" v-if="store.selectedCount > 0">{{ store.selectedCount }}</span>
      </button>

      <!-- Search -->
      <SearchInput v-model="store.searchQuery" />
    </div>

    <!-- Groups -->
    <div id="groups-scroll" class="groups-scroll">
      <div v-if="store.filteredGroups.length === 0" class="no-results">
        <template v-if="store.searchQuery">
          No groups match <em>"{{ store.searchQuery }}"</em>.
        </template>
        <template v-else>
          No groups match this filter.
        </template>
      </div>
      <DuplicateGroup
        v-for="group in store.filteredGroups"
        :key="group.entries.map(e => e.path).join('|')"
        :group="group"
      />
    </div>
  </template>

  <!-- Confirm delete dialog -->
  <Transition name="overlay-fade">
    <div class="overlay" v-if="showConfirm" @click.self="showConfirm = false">
      <div class="dialog">
        <div class="dialog-icon">🗑️</div>

        <h3 class="dialog-title">Delete {{ store.selectedCount }} image{{ store.selectedCount !== 1 ? 's' : '' }}?</h3>

        <div class="dialog-files">
          <div
            v-for="path in previewPaths"
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
          <div v-if="store.selectedCount > DELETE_MAX_PREVIEW" class="dialog-file dialog-more">
            +{{ store.selectedCount - DELETE_MAX_PREVIEW }} more…
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
          <button class="btn btn-ghost" @click="showConfirm = false; deleteError = null">Cancel</button>
          <button class="btn btn-danger" @click="doDelete" :disabled="deleting">
            <span v-if="deleting && deleteProgress.total > 0">
              Deleting {{ deleteProgress.done }}/{{ deleteProgress.total }}…
            </span>
            <span v-else-if="deleting">Deleting…</span>
            <span v-else>Delete {{ store.selectedCount }} file{{ store.selectedCount !== 1 ? 's' : '' }}</span>
          </button>
        </div>
      </div>
    </div>
  </Transition>
  <ImageDetailPanel />
</template>

<script setup>
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useDuplicatesStore } from '../store/duplicates'
import DuplicateGroup from './DuplicateGroup.vue'
import ScanProgress from './ScanProgress.vue'
import ImageDetailPanel from './ImageDetailPanel.vue'
import SearchInput from './SearchInput.vue'
import { useThumbnailStore } from '../store/thumbnails'
import { fileName } from '../utils/formatters'
import { DELETE_MAX_PREVIEW } from '../constants'
import FailedFilesWarning from './FailedFilesWarning.vue'

const store        = useDuplicatesStore()
const thumbs       = useThumbnailStore()
const showConfirm  = ref(false)
const deleteError  = ref(null)

function focusFirstCard() {
  const first = document.querySelector('[data-card-path]')
  first?.focus()
  first?.scrollIntoView({ block: 'nearest' })
}

watch(() => store.scanDone, (done) => {
  if (done) setTimeout(focusFirstCard, 100)
})

watch(() => store.filteredGroups, () => {
  thumbs.clearThumbQueue()
})

function onWindowKeydown(e) {
  if (store.lightbox) return
  if (showConfirm.value) return
  const arrows = ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown']
  if (!arrows.includes(e.key)) return
  const tag = document.activeElement?.tagName
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return
  const active = document.activeElement
  if (!active?.dataset?.cardPath) {
    e.preventDefault()
    focusFirstCard()
  }
}

onMounted(()       => window.addEventListener('keydown', onWindowKeydown))
onBeforeUnmount(() => window.removeEventListener('keydown', onWindowKeydown))

const deleting       = ref(false)
const deleteProgress = ref({ done: 0, total: 0 })
const networkRoots   = ref(new Set())

function pathIsNetwork(p) {
  for (const root of networkRoots.value) {
    if (p.startsWith(root)) return true
  }
  return false
}

const hasNetworkFiles = computed(() =>
  [...store.selected].some(pathIsNetwork)
)
const hasMixed = computed(() =>
  hasNetworkFiles.value && [...store.selected].some(p => !pathIsNetwork(p))
)

const previewPaths = computed(() =>
  [...store.selected].slice(0, DELETE_MAX_PREVIEW)
)

async function confirmDelete() {
  if (store.selectedCount === 0) return
  try {
    const checks = await Promise.all(
      store.folders.map(f => invoke('is_network_path', { path: f }))
    )
    networkRoots.value = new Set(
      store.folders.filter((_, i) => checks[i])
    )
  } catch {
    networkRoots.value = new Set()
  }
  showConfirm.value = true
}

async function doDelete() {
  deleting.value = true
  deleteError.value = null
  deleteProgress.value = { done: 0, total: store.selectedCount }

  const unlisten = await listen('delete_progress', (e) => {
    deleteProgress.value = e.payload
  })

  try {
    await store.deleteSelected()
    showConfirm.value = false
    store.multiSelect = false
  } catch (e) {
    deleteError.value = `Delete failed: ${e}`
  } finally {
    unlisten()
    deleting.value = false
    deleteProgress.value = { done: 0, total: 0 }
  }
}
</script>

<style scoped>
/* ── Empty state ── */
.empty-state {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-muted);
  font-size: var(--font-size-md);
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
.btn-danger { background: var(--color-danger); color: #fff; border: none; }
.btn-danger:hover:not(:disabled) { background: var(--color-danger-hover); }
.btn:disabled { opacity: 0.4; cursor: not-allowed; }

.badge {
  background: rgba(255,255,255,0.25);
  border-radius: var(--border-radius-pill);
  padding: 1px 6px;
  font-size: 11px;
  font-weight: 600;
}

/* ── Groups scroll ── */
.groups-scroll {
  flex: 1;
  overflow-y: auto;
  /* Extra padding-bottom ensures the box-shadow glow of the last row
     is never clipped by the scroll container's overflow boundary. */
  padding: var(--space-4) var(--space-4) 32px var(--space-4);
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
  will-change: scroll-position;
}
.groups-scroll :deep(.group) {
  content-visibility: auto;
  contain-intrinsic-size: 0 280px;
}

.no-results {
  color: var(--text-muted);
  text-align: center;
  margin-top: var(--space-6);
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
}

.dialog-icon {
  font-size: 28px;
  text-align: center;
}

.dialog-title {
  font-size: var(--font-size-lg);
  font-weight: 600;
  color: var(--text-primary);
  text-align: center;
  margin: 0;
}

.dialog-files {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  padding: var(--space-2);
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.dialog-file {
  display: flex;
  align-items: center;
  font-size: 11px;
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  padding: 2px var(--space-1);
  border-radius: 3px;
}
.dialog-file:nth-child(odd) { background: rgba(255,255,255,0.03); }
.dialog-more {
  color: var(--text-muted);
  font-style: italic;
}

.file-dot {
  display: inline-block;
  width: 7px;
  height: 7px;
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

.dialog-warning--danger {
  color: var(--color-danger) !important;
  background: rgba(220, 53, 69, 0.08);
  padding: var(--space-2);
  border-radius: var(--border-radius-sm);
  border: 1px solid rgba(220, 53, 69, 0.25);
}

.dialog-warning {
  font-size: var(--font-size-xs);
  color: var(--text-muted);
  text-align: center;
  line-height: 1.5;
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
