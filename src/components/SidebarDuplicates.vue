<template>
  <SidebarDivider />

  <SidebarScanButton
    :scanning="store.scanning"
    :stopping="store.stopping"
    :disabled="store.folders.length === 0"
    @start="store.startScan()"
    @stop="store.stopScan()"
  />

  <SidebarDivider />

  <!-- Folder list -->
  <FolderSection :folders="store.folders" @add="pickFolder" @remove="store.removeFolder" />

  <SidebarDivider />

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
    <SidebarDivider />
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
    <SidebarDivider />
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
  <SidebarHistory
    :entries="history.entries"
    :folder-status="history.folderStatus"
    :missing-folders="history.missingFolders"
    :active-id="store.scanDone ? store.activeHistoryEntryId : null"
    :disabled="store.scanning"
    @select="loadFromHistory"
    @remove="history.removeEntry"
  >
    <template #stats="{ entry }">
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
    </template>
  </SidebarHistory>
</template>

<script setup>
import { watch } from 'vue'
import { useDuplicatesStore } from '../store/duplicates'
import { useDuplicatesHistoryStore } from '../store/duplicatesHistory'
import { useFolderPicker } from '../composables/useFolderPicker'
import SelectChevron from './SelectChevron.vue'
import FolderSection from './FolderSection.vue'
import SidebarScanButton from './SidebarScanButton.vue'
import SidebarHistory from './SidebarHistory.vue'
import SidebarDivider from './SidebarDivider.vue'
import '../styles/sidebar-shared.css'

const store   = useDuplicatesStore()
const history = useDuplicatesHistoryStore()
const { pickDirectory } = useFolderPicker()

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

function pickFolder() {
  pickDirectory(path => store.addFolder(path))
}
</script>

<style scoped>
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

/* ── History badges (duplicates-only) ── */
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
</style>
