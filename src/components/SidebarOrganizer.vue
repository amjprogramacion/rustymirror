<template>
  <div class="sidebar-divider" />

  <SidebarScanButton
    :scanning="org.scanning"
    :disabled="!org.folders.length"
    @start="org.runScan()"
    @stop="org.stop()"
  />

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

    <!-- Output directory + folder template — shown when !onlyRename -->
    <template v-if="!org.config.onlyRename">
      <div class="config-row config-dir">
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

      <!-- Folder template -->
      <div class="config-block">
        <span class="config-label">Folder template</span>
        <input
          ref="folderTemplateInputEl"
          class="template-input"
          type="text"
          :value="org.config.folderTemplate"
          @input="org.updateConfig({ folderTemplate: $event.target.value })"
          placeholder="REORDENADAS/{year}/{device}/{month_dir}"
          spellcheck="false"
          autocomplete="off"
        />
        <div class="template-tags">
          <button
            v-for="tag in FOLDER_TAGS"
            :key="tag.token"
            class="tag-chip"
            :title="tag.label"
            @mousedown.prevent
            @click="insertFolderTag(tag.token)"
          >{{ tag.token }}</button>
        </div>
        <span class="template-preview">{{ folderTemplatePreview }}</span>
      </div>
    </template>

    <!-- Renaming template -->
    <div class="config-block">
      <span class="config-label">Renaming template</span>
      <input
        ref="templateInputEl"
        class="template-input"
        type="text"
        :value="org.config.renameTemplate"
        @input="org.updateConfig({ renameTemplate: $event.target.value })"
        placeholder="{type}_{date}_{time}_{4hex_uid}"
        spellcheck="false"
        autocomplete="off"
      />
      <div class="template-tags">
        <button
          v-for="tag in TEMPLATE_TAGS"
          :key="tag.token"
          class="tag-chip"
          :title="tag.label"
          @mousedown.prevent
          @click="insertTag(tag.token)"
        >{{ tag.token }}</button>
      </div>
      <span class="template-preview">{{ templatePreview }}</span>
    </div>

  </section>

  <!-- Recent scans -->
  <SidebarHistory
    :entries="orgHistory.entries"
    :folder-status="orgHistory.folderStatus"
    :missing-folders="orgHistory.missingFolders"
    :active-id="org.activeHistoryEntryId"
    :disabled="org.scanning"
    @select="org.loadFromHistory"
    @remove="orgHistory.removeEntry"
  >
    <template #stats="{ entry }">
      <span class="history-stats">
        {{ entry.images ?? 0 }} img · {{ entry.videos ?? 0 }} vid
      </span>
    </template>
  </SidebarHistory>
</template>

<script setup>
import { ref, computed, nextTick } from 'vue'
import { useOrganizerStore } from '../store/organizer'
import { useOrganizerHistoryStore } from '../store/organizerHistory'
import { useFolderPicker } from '../composables/useFolderPicker'
import { shortPath } from '../utils/formatters'
import FolderSection from './FolderSection.vue'
import SelectChevron from './SelectChevron.vue'
import SidebarScanButton from './SidebarScanButton.vue'
import SidebarHistory from './SidebarHistory.vue'
import '../styles/sidebar-shared.css'

const org = useOrganizerStore()
const orgHistory = useOrganizerHistoryStore()
const { pickDirectory } = useFolderPicker()

function pickFolder() {
  pickDirectory(path => org.addFolder(path))
}

function pickOutputDir() {
  pickDirectory(path => org.updateConfig({ outputDirectory: path }))
}

// ── Folder template ───────────────────────────────────────────────────────────

const folderTemplateInputEl = ref(null)

const FOLDER_TAGS = [
  { token: '{year}',       label: 'YYYY' },
  { token: '{month}',      label: 'MM' },
  { token: '{month_name}', label: 'Month name (ENERO, FEBRERO…)' },
  { token: '{month_dir}',  label: 'MM - MONTHNAME combined' },
  { token: '{device}',     label: 'Device / camera name' },
  { token: '{day}',        label: 'DD' },
]

function insertFolderTag(token) {
  const input = folderTemplateInputEl.value
  if (!input) return
  const start = input.selectionStart ?? input.value.length
  const end   = input.selectionEnd   ?? input.value.length
  const current = org.config.folderTemplate ?? ''
  org.updateConfig({ folderTemplate: current.slice(0, start) + token + current.slice(end) })
  nextTick(() => {
    input.focus()
    const pos = start + token.length
    input.setSelectionRange(pos, pos)
  })
}

const MONTHS_PREVIEW = ['ENERO','FEBRERO','MARZO','ABRIL','MAYO','JUNIO','JULIO','AGOSTO','SEPTIEMBRE','OCTUBRE','NOVIEMBRE','DICIEMBRE']

const folderTemplatePreview = computed(() => {
  const tpl = org.config.folderTemplate || 'REORDENADAS/{year}/{device}/{month_dir}'
  return tpl
    .replace('{year}',       '2023')
    .replace('{month}',      '12')
    .replace('{month_name}', MONTHS_PREVIEW[11])
    .replace('{month_dir}',  '12 - ' + MONTHS_PREVIEW[11])
    .replace('{device}',     'Pixel 7')
    .replace('{day}',        '01')
})

// ── Renaming template ──────────────────────────────────────────────────────────

const templateInputEl = ref(null)

const TEMPLATE_TAGS = [
  { token: '{type}',        label: 'IMG or VID' },
  { token: '{date}',        label: 'YYYYMMDD' },
  { token: '{time}',        label: 'HHMMSS' },
  { token: '{year}',        label: 'YYYY' },
  { token: '{month}',       label: 'MM' },
  { token: '{day}',         label: 'DD' },
  { token: '{hour}',        label: 'HH' },
  { token: '{min}',         label: 'mm' },
  { token: '{sec}',         label: 'ss' },
  { token: '{4hex_uid}',    label: 'Sequential hex ID — change N for desired length' },
  { token: '{4crypto_uid}', label: 'N random alphanumeric chars — change N for desired length' },
]

function insertTag(token) {
  const input = templateInputEl.value
  if (!input) return
  const start = input.selectionStart ?? input.value.length
  const end   = input.selectionEnd   ?? input.value.length
  const current = org.config.renameTemplate ?? ''
  org.updateConfig({ renameTemplate: current.slice(0, start) + token + current.slice(end) })
  nextTick(() => {
    input.focus()
    const pos = start + token.length
    input.setSelectionRange(pos, pos)
  })
}

const templatePreview = computed(() => {
  const tpl = org.config.renameTemplate || '{type}_{date}_{time}_{4hex_uid}'
  const withFixed = tpl
    .replace('{type}',   'IMG')
    .replace('{date}',   '20231201')
    .replace('{time}',   '143022')
    .replace('{year}',   '2023')
    .replace('{month}',  '12')
    .replace('{day}',    '01')
    .replace('{hour}',   '14')
    .replace('{min}',    '30')
    .replace('{sec}',    '22')
  const withHex = withFixed.replace(/\{(\d+)hex_uid\}/g, (_, n) => {
    const len = Math.min(Math.max(parseInt(n, 10) || 4, 1), 32)
    return '0A1B3C2D'.slice(0, len).padEnd(len, '0')
  })
  const withCrypto = withHex.replace(/\{(\d+)crypto_uid\}/g, (_, n) => {
    const len = Math.min(Math.max(parseInt(n, 10) || 4, 1), 64)
    return 'AB3XKM9Z'.slice(0, len).padEnd(len, 'X')
  })
  return withCrypto + '.jpg'
})
</script>

<style scoped>
/* ── Config rows (organizer-only) ── */
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

/* ── Template block ── */
.config-block {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.template-input {
  width: 100%;
  box-sizing: border-box;
  padding: 4px 8px;
  font-size: 11px;
  font-family: var(--font-mono, monospace);
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-primary);
  outline: none;
  transition: border-color var(--transition);
}
.template-input:focus { border-color: var(--color-accent); }
.template-input::placeholder { color: var(--text-muted); }
.template-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}
.tag-chip {
  padding: 2px 6px;
  font-size: 10px;
  font-family: var(--font-mono, monospace);
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--color-accent);
  cursor: pointer;
  transition: background var(--transition), border-color var(--transition);
  line-height: 1.4;
}
.tag-chip:hover {
  background: color-mix(in srgb, var(--color-accent) 12%, transparent);
  border-color: var(--color-accent);
}
.template-preview {
  font-size: 10px;
  font-family: var(--font-mono, monospace);
  color: var(--text-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
</style>
