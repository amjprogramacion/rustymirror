<template>
  <SidebarDivider />

  <SidebarScanButton
    :scanning="org.scanning"
    :stopping="org.stopping"
    :disabled="!org.folders.length"
    @start="org.runScan()"
    @stop="org.stop()"
  />

  <SidebarDivider />

  <!-- Folder list -->
  <FolderSection :folders="org.folders" @add="pickFolder" @remove="org.removeFolder" />

  <!-- Sort — only after scan -->
  <template v-if="org.scanResult">
    <SidebarDivider />
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

  <SidebarDivider />

  <!-- Config -->
  <section class="sidebar-section">
    <p class="section-label">Options</p>

    <!-- Date priority chain (drag-and-drop) -->
    <div class="config-block">
      <span class="config-label">Date source priority</span>
      <div class="priority-list">
        <div
          v-for="(item, idx) in priorityItems"
          :key="item.id"
          :data-pidx="idx"
          class="priority-item"
          :class="{
            'priority-item--locked':        item.locked,
            'priority-item--dragging':      dragIdx === idx,
            'priority-item--line-above':    dropLine === idx      && dragIdx !== -1 && dragIdx !== idx && dragIdx !== idx - 1,
            'priority-item--line-below':    dropLine === idx + 1  && dragIdx !== -1 && dragIdx !== idx && dragIdx !== idx + 1 && idx === priorityItems.length - 2,
          }"
          @pointerdown="onPointerDown(idx, $event)"
        >
          <span class="priority-handle" aria-hidden="true">{{ item.locked ? '·' : '⠿' }}</span>
          <button class="priority-info-btn" @pointerdown.stop @click.stop="openInfo(item.id)">i</button>
          <span class="priority-label">{{ item.label }}</span>
          <span class="priority-badge">{{ idx + 1 }}</span>
        </div>
      </div>
    </div>

    <!-- Only rename + output dir + folder template — shown only after scan -->
    <template v-if="org.scanResult">
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
          placeholder="{year}/{device}/{month_dir}"
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

    </template><!-- /v-if="org.scanResult" -->

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

  <!-- Date source info modal -->
  <Teleport to="body">
    <div v-if="infoItem" class="priority-modal-overlay" @click="infoItem = null">
      <div class="priority-modal" @click.stop>
        <div class="priority-modal-header">
          <span class="priority-modal-title">{{ PRIORITY_INFO[infoItem].title }}</span>
          <button class="priority-modal-close" @click="infoItem = null">✕</button>
        </div>
        <p class="priority-modal-body">{{ PRIORITY_INFO[infoItem].body }}</p>
        <ul v-if="PRIORITY_INFO[infoItem].examples" class="priority-modal-examples">
          <li v-for="ex in PRIORITY_INFO[infoItem].examples" :key="ex">{{ ex }}</li>
        </ul>
      </div>
    </div>
  </Teleport>
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
import SidebarDivider from './SidebarDivider.vue'
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

// ── Date priority drag-and-drop ───────────────────────────────────────────────

const PRIORITY_META = {
  exif:     { label: 'EXIF metadata' },
  filename: { label: 'Filename pattern' },
  modify:   { label: 'File date (mtime)' },
  fallback: { label: 'Fallback year' },
}

const PRIORITY_INFO = {
  exif: {
    title: 'EXIF metadata',
    body: 'Reads DateTimeOriginal, CreateDate, or MediaCreateDate from the file via ExifTool. If multiple tags are present, picks the oldest. Most reliable source for photos taken with cameras or phones.',
    examples: ['DateTimeOriginal: 2023:05:21 14:30:00', 'CreateDate / MediaCreateDate (video)'],
  },
  filename: {
    title: 'Filename pattern',
    body: 'Scans the filename for recognisable date patterns. Covers the following formats:',
    examples: [
      '20230521_143000  →  full datetime',
      '2023-05-21-14-30-00  →  dashed datetime',
      '20230521_1430  →  datetime without seconds',
      'IMG_20230521_…  →  date only, time assigned sequentially',
      '-20230521-WA0001  →  WhatsApp export',
    ],
  },
  modify: {
    title: 'File date (mtime)',
    body: 'Uses the file system modification date (FileModifyDate). Unreliable if files have been copied, synced, or migrated — those operations reset this date. Use as a last resort before fallback.',
  },
  fallback: {
    title: 'Fallback year',
    body: 'When no other source finds a date, the configured fallback year is used with the current clock time. This slot is always last and cannot be reordered.',
  },
}

const infoItem = ref(null)
function openInfo(id) { infoItem.value = id }

const priorityItems = computed(() => [
  ...org.config.datePriorityOrder.map(id => ({ id, ...PRIORITY_META[id], locked: false })),
  { id: 'fallback', ...PRIORITY_META.fallback, locked: true },
])

const dragIdx  = ref(-1)
const dropLine = ref(-1)   // insertion index: item will be placed BEFORE this position

function onPointerDown(idx, evt) {
  if (priorityItems.value[idx].locked) return
  evt.preventDefault()
  dragIdx.value  = idx
  dropLine.value = -1
  window.addEventListener('pointermove', _onPointerMove)
  window.addEventListener('pointerup',   _onPointerUp)
}

function _onPointerMove(evt) {
  const el = document.elementFromPoint(evt.clientX, evt.clientY)?.closest('[data-pidx]')
  if (!el) return
  const idx = parseInt(el.dataset.pidx)
  if (isNaN(idx)) return
  const maxOrdinal = org.config.datePriorityOrder.length  // 3
  if (idx >= maxOrdinal) {
    // hovering over fallback → line after last orderable item
    dropLine.value = maxOrdinal
    return
  }
  const rect = el.getBoundingClientRect()
  dropLine.value = evt.clientY < rect.top + rect.height / 2 ? idx : idx + 1
}

function _onPointerUp() {
  window.removeEventListener('pointermove', _onPointerMove)
  window.removeEventListener('pointerup',   _onPointerUp)
  const from = dragIdx.value
  const line = dropLine.value
  dragIdx.value  = -1
  dropLine.value = -1
  if (from === -1 || line === -1 || line === from || line === from + 1) return
  const order = [...org.config.datePriorityOrder]
  const [moved] = order.splice(from, 1)
  // line is in original-array terms; adjust for the removal
  const insertAt = line > from ? line - 1 : line
  order.splice(insertAt, 0, moved)
  org.updateConfig({ datePriorityOrder: order })
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
  const tpl = org.config.folderTemplate || '{year}/{device}/{month_dir}'
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

/* ── Date priority list ── */
.priority-list {
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.priority-item {
  position: relative;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 7px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  cursor: grab;
  user-select: none;
  touch-action: none;
  transition: background var(--transition), border-color var(--transition), opacity var(--transition);
}
.priority-item--locked {
  cursor: default;
  opacity: 0.55;
}
.priority-item--dragging {
  opacity: 0.35;
  cursor: grabbing;
}
.priority-item--line-above::before,
.priority-item--line-below::after {
  content: '';
  position: absolute;
  left: 0;
  right: 0;
  height: 2px;
  background: var(--color-accent);
  border-radius: 1px;
}
.priority-item--line-above::before { top: -3px; }
.priority-item--line-below::after  { bottom: -3px; }
.priority-handle {
  font-size: 13px;
  color: var(--text-muted);
  line-height: 1;
  flex-shrink: 0;
  width: 12px;
  text-align: center;
}
.priority-label {
  flex: 1;
  font-size: 11px;
  color: var(--text-secondary);
}
.priority-badge {
  font-size: 10px;
  color: var(--text-muted);
  flex-shrink: 0;
}

/* ── Info button ── */
.priority-info-btn {
  flex-shrink: 0;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  border: 1px solid var(--border-color);
  background: none;
  color: var(--text-muted);
  font-size: 9px;
  font-style: italic;
  font-family: Georgia, serif;
  font-weight: bold;
  line-height: 1;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  transition: border-color var(--transition), color var(--transition);
}
.priority-info-btn:hover {
  border-color: var(--color-accent);
  color: var(--color-accent);
}

/* ── Info modal ── */
.priority-modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.45);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 9999;
}
.priority-modal {
  background: var(--bg-surface, var(--bg-card));
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius);
  padding: 16px 18px;
  max-width: 340px;
  width: calc(100vw - 48px);
  box-shadow: 0 8px 32px rgba(0,0,0,0.4);
}
.priority-modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 10px;
}
.priority-modal-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}
.priority-modal-close {
  background: none;
  border: none;
  color: var(--text-muted);
  font-size: 12px;
  cursor: pointer;
  padding: 2px 4px;
  line-height: 1;
  border-radius: var(--border-radius-sm);
  transition: color var(--transition), background var(--transition);
}
.priority-modal-close:hover {
  color: var(--text-primary);
  background: var(--bg-card-hover);
}
.priority-modal-body {
  font-size: 12px;
  color: var(--text-secondary);
  line-height: 1.55;
  margin: 0 0 8px;
}
.priority-modal-examples {
  margin: 0;
  padding: 0 0 0 4px;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.priority-modal-examples li {
  font-size: 11px;
  font-family: var(--font-mono, monospace);
  color: var(--text-muted);
  padding: 2px 6px;
  background: var(--bg-card);
  border-radius: var(--border-radius-sm);
  border-left: 2px solid var(--color-accent);
}
</style>
