<template>
  <div class="sidebar-divider" />

  <!-- Scan button -->
  <button
    class="btn-scan"
    :class="org.scanning ? 'btn-danger' : 'btn-success'"
    :disabled="!org.scanning && !org.folders.length"
    @click="org.scanning ? org.stop() : org.runScan()"
  >
    {{ org.scanning ? 'Stop scan' : 'Scan' }}
  </button>

  <div class="sidebar-divider" />

  <!-- Folder list -->
  <FolderSection :folders="org.folders" @add="pickFolder" @remove="org.removeFolder" />

  <!-- Sort — only after scan -->
  <template v-if="org.scanResult">
    <div class="sidebar-divider" />
    <section class="sidebar-section">
      <p class="section-label">Sort</p>
      <div class="sort-selects">
        <div class="select-field">
          <select class="sort-select filter-select" v-model="org.sortBy">
            <option value="filename">Filename</option>
            <option value="type">Type</option>
            <option value="date">Date</option>
          </select>
          <SelectChevron />
        </div>
        <div class="select-field">
          <select class="sort-select filter-select" v-model="org.sortDir">
            <option value="asc">Ascending</option>
            <option value="desc">Descending</option>
          </select>
          <SelectChevron />
        </div>
      </div>
    </section>
  </template>

  <div class="sidebar-divider" />

  <!-- Config -->
  <section class="sidebar-section">
    <p class="section-label">Options</p>

    <!-- Date priority -->
    <div class="config-row">
      <span class="config-label">Date source</span>
      <div class="toggle-group">
        <button
          class="toggle-btn"
          :class="{ active: org.config.datePriority === 'exif' }"
          @click="org.updateConfig({ datePriority: 'exif' })"
        >EXIF</button>
        <button
          class="toggle-btn"
          :class="{ active: org.config.datePriority === 'filename' }"
          @click="org.updateConfig({ datePriority: 'filename' })"
        >Filename</button>
      </div>
    </div>

    <!-- Only rename -->
    <div class="config-row">
      <span class="config-label">Only rename (don't move)</span>
      <label class="toggle">
        <input type="checkbox" :checked="org.config.onlyRename"
          @change="org.updateConfig({ onlyRename: $event.target.checked })" />
        <span class="toggle-track"><span class="toggle-thumb" /></span>
      </label>
    </div>

    <!-- Output directory — shown when !onlyRename -->
    <div v-if="!org.config.onlyRename" class="config-row config-dir">
      <span class="config-label">Output folder</span>
      <div class="dir-picker">
        <span
          class="dir-value"
          :class="{ placeholder: !org.config.outputDirectory }"
          :title="org.config.outputDirectory"
        >
          {{ org.config.outputDirectory ? shortPath(org.config.outputDirectory) : 'Not set' }}
        </span>
        <button class="btn-pick-dir" @click="pickOutputDir" title="Browse">…</button>
      </div>
    </div>

  </section>

</template>

<script setup>
import { open } from '@tauri-apps/plugin-dialog'
import { useOrganizerStore } from '../store/organizer'
import FolderSection from './FolderSection.vue'
import SelectChevron from './SelectChevron.vue'

const org = useOrganizerStore()

async function pickFolder() {
  const path = await open({ directory: true, multiple: false })
  if (path) org.addFolder(path)
}

async function pickOutputDir() {
  const path = await open({ directory: true, multiple: false })
  if (path) org.updateConfig({ outputDirectory: path })
}

function shortPath(p) {
  const parts = p.replace(/\\/g, '/').split('/')
  if (parts.length <= 2) return p
  return '…/' + parts.slice(-2).join('/')
}
</script>

<style scoped>
.sidebar-divider {
  height: 1px;
  background: var(--sidebar-border);
  margin: 0;
}

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

/* ── Config rows ── */
.config-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
}
.config-label {
  font-size: 11px;
  color: var(--text-muted);
  flex-shrink: 0;
}
/* ── Toggle group ── */
.toggle-group {
  display: flex;
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  overflow: hidden;
}
.toggle-btn {
  padding: 3px 9px;
  font-size: 11px;
  background: none;
  color: var(--text-muted);
  border: none;
  cursor: pointer;
  transition: background var(--transition), color var(--transition);
}
.toggle-btn:first-child { border-right: 1px solid var(--border-color); }
.toggle-btn.active {
  background: var(--color-accent);
  color: #fff;
}

/* ── Dir picker ── */
.config-dir { flex-direction: column; align-items: flex-start; gap: 4px; }
.dir-picker {
  display: flex;
  align-items: center;
  gap: 4px;
  width: 100%;
}
.dir-value {
  flex: 1;
  font-size: 11px;
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  padding: 3px 6px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
}
.dir-value.placeholder { color: var(--text-muted); }
.btn-pick-dir {
  flex-shrink: 0;
  padding: 3px 8px;
  font-size: 12px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-secondary);
  cursor: pointer;
  transition: background var(--transition);
}
.btn-pick-dir:hover { background: var(--bg-card-hover); }

/* ── Year input ── */
.year-input {
  width: 64px;
  padding: 3px 6px;
  font-size: 12px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-primary);
  outline: none;
  text-align: center;
}
.year-input:focus { border-color: var(--color-accent); }

/* ── Toggle ── */
.toggle {
  position: relative;
  display: inline-flex;
  align-items: center;
  cursor: pointer;
  flex-shrink: 0;
}
.toggle input {
  position: absolute;
  opacity: 0;
  width: 0;
  height: 0;
}
.toggle-track {
  width: 32px;
  height: 18px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-pill);
  transition: background var(--transition), border-color var(--transition);
  display: flex;
  align-items: center;
  padding: 2px;
}
.toggle input:checked + .toggle-track {
  background: var(--color-accent);
  border-color: var(--color-accent);
}
.toggle-thumb {
  width: 12px;
  height: 12px;
  background: var(--text-muted);
  border-radius: 50%;
  transition: transform var(--transition), background var(--transition);
}
.toggle input:checked + .toggle-track .toggle-thumb {
  transform: translateX(14px);
  background: #fff;
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
.btn:disabled { opacity: 0.35; cursor: not-allowed; }
.btn-full { width: 100%; }

.btn-primary {
  background: var(--color-accent);
  color: #fff;
}
.btn-primary:hover:not(:disabled) { filter: brightness(1.12); }

.btn-success {
  background: var(--color-success);
  color: #fff;
}
.btn-success:hover:not(:disabled) { background: var(--color-success-hover); }

.btn-secondary {
  background: var(--bg-card);
  color: var(--text-secondary);
  border: 1px solid var(--border-color);
}
.btn-secondary:hover:not(:disabled) { background: var(--bg-card-hover); }

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
</style>
