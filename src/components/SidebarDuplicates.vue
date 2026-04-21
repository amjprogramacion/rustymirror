<template>
  <div class="sidebar-divider" />

  <!-- Scan button -->
  <button
    class="btn-scan"
    :class="store.scanning ? 'btn-danger' : 'btn-success'"
    :disabled="!store.scanning && store.folders.length === 0"
    @click="store.scanning ? store.stopScan() : store.startScan()"
  >
    {{ store.scanning ? 'Stop scan' : 'Scan' }}
  </button>

  <div class="sidebar-divider" />

  <!-- Folder list -->
  <FolderSection :folders="store.folders" @add="pickFolder" @remove="store.removeFolder" />

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

  <!-- Sort + Filter — only after scan -->
  <template v-if="store.scanDone">
    <div class="sidebar-divider" />
    <section class="sidebar-section">
      <p class="section-label">Sort</p>
      <div class="sort-selects">
        <div class="select-field">
          <select class="sort-select filter-select" v-model="store.dupSortBy">
            <option value="group">Group</option>
            <option value="date">Date</option>
            <option value="title">Title</option>
          </select>
          <SelectChevron />
        </div>
        <div class="select-field">
          <select class="sort-select filter-select" v-model="store.dupSortDir">
            <option value="asc">Ascending</option>
            <option value="desc">Descending</option>
          </select>
          <SelectChevron />
        </div>
      </div>
    </section>
    <div class="sidebar-divider" />
    <section class="sidebar-section">
      <p class="section-label">Filter</p>
      <div class="filter-labeled-row">
        <span class="filter-label">Group</span>
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
      </div>
      <div class="filter-labeled-row" v-if="store.availableExtensions.length > 1">
        <span class="filter-label">Extension</span>
        <select class="sort-select filter-select" v-model="store.extFilter">
          <option value="">All</option>
          <option v-for="ext in store.availableExtensions" :key="ext" :value="ext">
            .{{ ext }}
          </option>
        </select>
      </div>
    </section>
  </template>

  <!-- Recent scans -->
  <template v-if="history.entries.length > 0">
    <div class="sidebar-divider" />
    <section class="sidebar-section history-section">
      <p class="section-label">Recent scans</p>
      <div class="history-entries-scroll">
        <div
          v-for="entry in history.entries"
          :key="entry.id"
          class="history-entry"
          :class="{
            disabled: store.scanning,
            active: isActiveEntry(entry),
            'history-entry--missing': history.folderStatus[entry.id] === 'missing',
            'history-entry--partial': history.folderStatus[entry.id] === 'partial',
          }"
          @click="loadFromHistory(entry)"
          :title="entry.folders.join('\n')"
        >
          <div class="history-date">
            {{ formatLocalDate(entry.date) }}<span v-if="formatDuration(entry.durationMs)" class="history-duration">&nbsp;({{ formatDuration(entry.durationMs) }})</span>
          </div>
          <div class="history-folders">
            <span
              v-for="f in entry.folders" :key="f"
              class="history-folder"
              :class="{ 'history-folder--missing': history.missingFolders[entry.id]?.includes(f) }"
              :title="f"
            >{{ shortPath(f) }}</span>
          </div>
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
              <span v-if="entry.crossDatePhash !== false" class="history-cross-badge" title="Cross-date similarity (phase 5)">
                <svg viewBox="0 0 11 11" fill="none" xmlns="http://www.w3.org/2000/svg">
                  <path d="M1 5.5h9M5.5 1v9M2.5 2.5l6 6M8.5 2.5l-6 6" stroke="#7ecfff" stroke-width="1.5" stroke-linecap="round"/>
                </svg>
              </span>
              <span class="history-threshold">{{ entry.threshold ?? 90 }}%</span>
            </div>
          </div>
          <span
            v-if="history.folderStatus[entry.id] === 'missing' || history.folderStatus[entry.id] === 'partial'"
            class="folder-alert"
            :title="history.folderStatus[entry.id] === 'missing' ? 'Folder no longer exists' : 'Some folders no longer exist'"
          >
            <IconWarning />
          </span>
          <button
            class="history-remove"
            title="Remove from history"
            @click.stop="history.removeEntry(entry.id)"
          >
            <IconClose />
          </button>
        </div>
      </div>
    </section>
  </template>
</template>

<script setup>
import { watch } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { useDuplicatesStore } from '../store/duplicates'
import { useDuplicatesHistoryStore } from '../store/duplicatesHistory'
import { shortPath, formatLocalDate, formatDuration } from '../utils/formatters'
import SelectChevron from './SelectChevron.vue'
import FolderSection from './FolderSection.vue'
import IconWarning from './IconWarning.vue'
import IconClose from './IconClose.vue'

const store   = useDuplicatesStore()
const history = useDuplicatesHistoryStore()

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

function isActiveEntry(entry) {
  return store.scanDone && entry.id === store.activeHistoryEntryId
}

function loadFromHistory(entry) {
  if (store.scanning) return
  const entryThreshold = entry.threshold ?? 90
  store.folders = [...entry.folders]
  store.similarityThreshold = entryThreshold
  if (entry.id === store.activeHistoryEntryId) return
  store.fastModeOverride      = entry.fastMode      ?? false
  store.crossDatePhashOverride = entry.crossDatePhash ?? true
  store.scanLabel = 'Loading scan…'
  store.startScan()
}

async function pickFolder() {
  const path = await open({ directory: true, multiple: false })
  if (path) store.addFolder(path)
}
</script>

<style scoped>
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
  display: flex;
  align-items: center;
}

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

/* ── Scan button ── */
.btn-scan {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  padding: 9px var(--space-3);
  border-radius: 0;
  font-size: var(--font-size-sm);
  font-weight: 500;
  transition: background var(--transition), opacity var(--transition);
}
.btn-scan:disabled { opacity: 0.35; cursor: not-allowed; }

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

/* ── Sort selects ── */
.sort-selects {
  display: flex;
  flex-direction: row;
  gap: 6px;
}

.sort-select {
  width: 100%;
  padding: 5px 8px;
  font-size: var(--font-size-xs);
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-primary);
  cursor: pointer;
  outline: none;
  appearance: auto;
  transition: border-color var(--transition);
}
.sort-select:focus { border-color: var(--color-accent); }

.filter-select {
  appearance: none;
  padding-right: 28px;
}

.select-field {
  position: relative;
  flex: 1;
  display: flex;
  align-items: center;
}

.select-field :deep(.select-chevron) {
  position: absolute;
  right: 8px;
}

.select-field .sort-select {
  width: 100%;
  padding-right: 28px;
}

/* ── Filter section ── */
.filter-labeled-row {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-bottom: 8px;
}
.filter-labeled-row:last-child { margin-bottom: 0; }

.filter-label {
  font-size: 10px;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.04em;
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

.pill.active .pill-count { background: #092a4d; }

/* ── History ── */
.history-section {
  flex: 1 1 0;
  min-height: 0;
  overflow: hidden;
}

.history-entries-scroll {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding-top: var(--space-1);
}

.history-entry {
  position: relative;
  padding: var(--space-2);
  border-radius: var(--border-radius-sm);
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  cursor: pointer;
  transition: background var(--transition), border-color var(--transition);
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
.history-entry--missing { background: rgba(220, 53, 69, 0.08); border-color: rgba(220, 53, 69, 0.25); }
.history-entry--partial { background: rgba(245, 197, 66, 0.08); border-color: rgba(245, 197, 66, 0.25); }
.history-entry--missing:hover:not(.disabled):not(.active) { background: rgba(220, 53, 69, 0.15); border-color: rgba(220, 53, 69, 0.25); }
.history-entry--partial:hover:not(.disabled):not(.active) { background: rgba(245, 197, 66, 0.15); border-color: rgba(245, 197, 66, 0.25); }

.history-date {
  font-size: 10px;
  color: var(--text-muted);
}

.history-duration { opacity: 0.7; }

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
.history-entry--partial .history-folder--missing { color: rgb(245, 197, 66); }
.history-entry--missing .history-folder--missing { color: rgb(220, 53, 69); }

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

.history-fast-badge,
.history-cross-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  background: transparent;
  border-radius: 50%;
}

.history-fast-badge svg  { width: 7px;  height: 11px; transform: rotate(15deg); }
.history-cross-badge svg { width: 11px; height: 11px; }

.history-remove {
  position: absolute;
  top: 4px;
  right: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  padding: 0;
  background: transparent;
  border: none;
  border-radius: 50%;
  color: var(--text-muted);
  cursor: pointer;
  transition: background var(--transition), color var(--transition);
}
.history-remove svg { width: 10px; height: 10px; display: block; }
.history-remove:hover { background: var(--color-danger); color: #fff; }

.folder-alert {
  position: absolute;
  top: 4px;
  right: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
}
.folder-alert svg { width: 10px; height: 10px; display: block; }
.history-entry--missing .folder-alert { color: rgb(220, 53, 69); }
.history-entry--partial .folder-alert { color: rgb(245, 197, 66); }
</style>
