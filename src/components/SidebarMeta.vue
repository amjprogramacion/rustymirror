<template>
  <SidebarDivider />

  <SidebarScanButton
    :scanning="meta.scanning || meta.geocoding"
    :stopping="meta.stopping"
    :disabled="meta.folders.length === 0"
    @start="meta.startScan()"
    @stop="meta.stopScan()"
  />

  <SidebarDivider />

  <!-- Folder list -->
  <FolderSection :folders="meta.folders" @add="pickMetaFolder" @remove="meta.removeFolder" />

  <template v-if="meta.scanDone">
    <SidebarDivider />

    <!-- Sorting -->
    <section class="sidebar-section">
      <p class="section-label">
        Sort
        <span v-if="meta.geocoding && meta.sortBy === 'location'" class="sort-geocoding-hint">
          · fetching locations…
        </span>
      </p>
      <div class="sort-selects">
        <div class="select-field">
          <select class="sort-select filter-select" v-model="meta.sortBy">
            <option v-for="opt in sortOptions" :key="opt.key" :value="opt.key">{{ opt.label }}</option>
          </select>
          <SelectChevron />
        </div>
        <div class="select-field">
          <select class="sort-select filter-select" v-model="meta.sortDir">
            <option value="asc">Ascending</option>
            <option value="desc">Descending</option>
          </select>
          <SelectChevron />
        </div>
      </div>
    </section>

    <SidebarDivider />

    <!-- Filtering -->
    <section class="sidebar-section">
      <p class="section-label">
        Filter
        <button
          v-if="meta.filterDateFrom || meta.filterDateTo || meta.filterLocation || meta.filterDevice"
          class="filter-clear-all"
          @click="meta.filterDateFrom = ''; meta.filterDateTo = ''; meta.filterLocation = ''; meta.filterDevice = ''"
        >clear all</button>
      </p>

      <!-- Date range -->
      <div class="filter-labeled-row">
        <span class="filter-label">Date</span>
        <div class="filter-row">
          <input type="date" class="filter-input" v-model="meta.filterDateFrom" title="From" placeholder="From" :style="{ color: meta.filterDateFrom ? 'inherit' : 'transparent' }" />
          <input type="date" class="filter-input" v-model="meta.filterDateTo" title="To" placeholder="To" :style="{ color: meta.filterDateTo ? 'inherit' : 'transparent' }" />
          <button v-if="meta.filterDateFrom || meta.filterDateTo" class="filter-clear-sq" @click="meta.filterDateFrom = ''; meta.filterDateTo = ''" title="Clear">
            <ClearIcon />
          </button>
        </div>
      </div>

      <!-- Location -->
      <div class="filter-labeled-row">
        <span class="filter-label">Location</span>
        <div class="filter-row">
          <div class="select-field">
            <select class="sort-select filter-select" v-model="meta.filterLocation">
              <option value="">All</option>
              <option value="__no_location__">Without location</option>
              <option v-for="loc in meta.availableLocations" :key="loc" :value="loc">{{ loc }}</option>
            </select>
            <SelectChevron />
          </div>
          <button
            class="filter-fetch-sq"
            :disabled="meta.geocodingManual || meta.scanning"
            @click="meta.fetchLocationsManual()"
            title="Fetch locations"
          >
            <span class="fetch-spin" :class="{ spinning: meta.geocodingManual }">
              <RefreshIcon />
            </span>
          </button>
          <button v-if="meta.filterLocation" class="filter-clear-sq" @click="meta.filterLocation = ''" title="Clear">
            <ClearIcon />
          </button>
        </div>
      </div>

      <!-- Device -->
      <div class="filter-labeled-row">
        <span class="filter-label">Device</span>
        <div class="filter-row">
          <div class="select-field">
            <select class="sort-select filter-select" v-model="meta.filterDevice">
              <option value="">All</option>
              <option v-for="dev in meta.availableDevices" :key="dev" :value="dev">{{ meta.deviceAliases[dev] || dev }}</option>
            </select>
            <SelectChevron />
          </div>
          <button
            class="filter-fetch-sq"
            :disabled="meta.devicesManual || meta.scanning"
            @click="meta.fetchDevicesManual()"
            title="Refresh devices"
          >
            <span class="fetch-spin" :class="{ spinning: meta.devicesManual }">
              <RefreshIcon />
            </span>
          </button>
          <button v-if="meta.filterDevice" class="filter-clear-sq" @click="meta.filterDevice = ''" title="Clear">
            <ClearIcon />
          </button>
        </div>
      </div>
    </section>
  </template>

  <!-- Recent scans (metadata) -->
  <SidebarHistory
    :entries="metaHistory.entries"
    :folder-status="metaHistory.folderStatus"
    :missing-folders="metaHistory.missingFolders"
    :active-id="meta.activeHistoryEntryId"
    :disabled="meta.scanning"
    @select="meta.loadFromHistory"
    @remove="metaHistory.removeEntry"
  >
    <template #stats="{ entry }">
      <span class="history-stats">
        {{ entry.imageCount ?? 0 }} image{{ (entry.imageCount ?? 0) !== 1 ? 's' : '' }}
      </span>
    </template>
  </SidebarHistory>
</template>

<script setup>
import { useMetadataStore } from '../store/metadata'
import { useMetadataHistoryStore } from '../store/metadataHistory'
import { useFolderPicker } from '../composables/useFolderPicker'
import SelectChevron from './SelectChevron.vue'
import ClearIcon from './ClearIcon.vue'
import RefreshIcon from './RefreshIcon.vue'
import FolderSection from './FolderSection.vue'
import SidebarScanButton from './SidebarScanButton.vue'
import SidebarHistory from './SidebarHistory.vue'
import SidebarDivider from './SidebarDivider.vue'
import '../styles/sidebar-shared.css'

const meta        = useMetadataStore()
const metaHistory = useMetadataHistoryStore()
const { pickDirectory } = useFolderPicker()

const sortOptions = [
  { key: 'date',     label: 'Date'     },
  { key: 'location', label: 'Location' },
  { key: 'device',   label: 'Device'   },
]

function pickMetaFolder() {
  pickDirectory(path => meta.addFolder(path))
}
</script>

<style scoped>
/* ── Sort hint (metadata-only) ── */
.sort-geocoding-hint {
  font-size: 10px;
  color: var(--text-muted);
  font-style: italic;
  font-weight: 400;
  text-transform: none;
  letter-spacing: 0;
}

/* ── Filter controls (metadata-only) ── */
.filter-clear-all {
  margin-left: auto;
  background: none;
  border: none;
  padding: 0;
  font-size: var(--font-size-xs);
  color: var(--color-accent);
  cursor: pointer;
  text-transform: uppercase;
  letter-spacing: 0.6px;
}
.filter-clear-all:hover { opacity: 0.75; }

.filter-row {
  display: flex;
  align-items: center;
  gap: 4px;
}

.filter-row .filter-input {
  flex: 1;
  min-width: 0;
}

.filter-input {
  flex: 1;
  min-width: 0;
  padding: 4px 6px;
  font-size: 11px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-primary);
  outline: none;
  transition: border-color var(--transition);
  box-sizing: border-box;
}
.filter-input:focus { border-color: var(--color-accent); }
.filter-input::-webkit-calendar-picker-indicator { filter: invert(0.6); cursor: pointer; }

.filter-clear-sq {
  display: flex;
  align-items: center;
  justify-content: center;
  aspect-ratio: 1;
  align-self: stretch;
  flex-shrink: 0;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-muted);
  cursor: pointer;
  transition: background var(--transition), color var(--transition), border-color var(--transition);
}
.filter-clear-sq:hover {
  background: var(--color-danger);
  border-color: var(--color-danger);
  color: #fff;
}

.filter-fetch-sq {
  display: flex;
  align-items: center;
  justify-content: center;
  aspect-ratio: 1;
  align-self: stretch;
  flex-shrink: 0;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-muted);
  cursor: pointer;
  transition: background var(--transition), color var(--transition), border-color var(--transition);
}
.filter-fetch-sq:hover:not(:disabled) {
  background: var(--color-accent);
  border-color: var(--color-accent);
  color: #fff;
}
.filter-fetch-sq:disabled {
  opacity: 0.45;
  cursor: default;
}
.fetch-spin {
  display: flex;
  align-items: center;
  justify-content: center;
}
.fetch-spin.spinning {
  animation: spin 0.9s linear infinite;
}
@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
