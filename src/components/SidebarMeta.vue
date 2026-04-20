<template>
  <div class="sidebar-divider" />

  <!-- Scan button -->
  <button
    class="btn-scan"
    :class="(meta.scanning || meta.geocoding) ? 'btn-danger' : 'btn-success'"
    :disabled="!meta.scanning && !meta.geocoding && meta.folders.length === 0"
    @click="(meta.scanning || meta.geocoding) ? meta.stopScan() : meta.startScan()"
  >
    {{ meta.scanning ? 'Stop scan' : meta.geocoding ? 'Stop scan' : 'Scan' }}
  </button>

  <div class="sidebar-divider" />

  <!-- Folder list -->
  <FolderSection :folders="meta.folders" @add="pickMetaFolder" @remove="meta.removeFolder" />

  <template v-if="meta.scanDone">
    <div class="sidebar-divider" />

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

    <div class="sidebar-divider" />

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
              <option v-for="dev in meta.availableDevices" :key="dev" :value="dev">{{ dev }}</option>
            </select>
            <SelectChevron />
          </div>
          <button v-if="meta.filterDevice" class="filter-clear-sq" @click="meta.filterDevice = ''" title="Clear">
            <ClearIcon />
          </button>
        </div>
      </div>
    </section>
  </template>

  <!-- Recent scans (metadata) -->
  <template v-if="metaHistory.entries.length > 0">
    <div class="sidebar-divider" />
    <section class="sidebar-section history-section">
      <p class="section-label">Recent scans</p>
      <div class="history-entries-scroll">
        <div
          v-for="entry in metaHistory.entries"
          :key="entry.id"
          class="history-entry"
          :class="{
            disabled: meta.scanning,
            active: entry.id === meta.activeHistoryEntryId,
            'history-entry--missing': metaHistory.folderStatus[entry.id] === 'missing',
            'history-entry--partial': metaHistory.folderStatus[entry.id] === 'partial',
          }"
          @click="meta.loadFromHistory(entry)"
          :title="entry.folders.join('\n')"
        >
          <div class="history-date">
            {{ formatLocalDate(entry.date) }}<span v-if="formatDuration(entry.durationMs)" class="history-duration">&nbsp;({{ formatDuration(entry.durationMs) }})</span>
          </div>
          <div class="history-folders">
            <span v-for="f in entry.folders" :key="f" class="history-folder" :title="f">
              {{ shortPath(f) }}
            </span>
          </div>
          <div class="history-footer">
            <span class="history-stats">
              {{ entry.imageCount ?? 0 }} image{{ (entry.imageCount ?? 0) !== 1 ? 's' : '' }}
            </span>
          </div>
          <span
            v-if="metaHistory.folderStatus[entry.id] === 'missing' || metaHistory.folderStatus[entry.id] === 'partial'"
            class="folder-alert"
            :title="metaHistory.folderStatus[entry.id] === 'missing' ? 'Folder no longer exists' : 'Some folders no longer exist'"
          >
            <IconWarning />
          </span>
          <button
            class="history-remove"
            title="Remove from history"
            @click.stop="metaHistory.removeEntry(entry.id)"
          >
            <IconClose />
          </button>
        </div>
      </div>
    </section>
  </template>
</template>

<script setup>
import { open } from '@tauri-apps/plugin-dialog'
import { useMetadataStore } from '../store/metadata'
import { useMetadataHistoryStore } from '../store/metadataHistory'
import { shortPath, formatLocalDate, formatDuration } from '../utils/formatters'
import SelectChevron from './SelectChevron.vue'
import ClearIcon from './ClearIcon.vue'
import FolderSection from './FolderSection.vue'
import IconWarning from './IconWarning.vue'
import IconClose from './IconClose.vue'

const meta        = useMetadataStore()
const metaHistory = useMetadataHistoryStore()

const sortOptions = [
  { key: 'date',     label: 'Date'     },
  { key: 'location', label: 'Location' },
  { key: 'device',   label: 'Device'   },
]

async function pickMetaFolder() {
  const path = await open({ directory: true, multiple: false })
  if (path) meta.addFolder(path)
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
.sort-geocoding-hint {
  font-size: 10px;
  color: var(--text-muted);
  font-style: italic;
  font-weight: 400;
  text-transform: none;
  letter-spacing: 0;
}

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

/* ── Filter controls ── */
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
  width: 28px;
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

.history-date     { font-size: 10px; color: var(--text-muted); }
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
