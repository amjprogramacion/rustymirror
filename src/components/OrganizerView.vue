<template>
  <div class="organizer-view">

    <!-- Empty state -->
    <div v-if="!org.folders.length" class="empty-state">
      <p class="empty-title">No folders selected</p>
      <p class="empty-sub">Add folders in the sidebar and press <strong>Scan</strong> to load files.</p>
    </div>

    <template v-else>

      <!-- Error banner -->
      <div v-if="org.error" class="error-banner">
        {{ org.error }}
      </div>

      <!-- Scan results card -->
      <div v-if="org.scanResult" class="scan-result-card">
        <div class="donut-wrap">
          <svg viewBox="0 0 100 100" class="donut-svg">
            <!-- Track -->
            <circle class="donut-track" cx="50" cy="50" :r="R" />
            <!-- Images arc -->
            <circle
              v-if="donut.images > 0"
              class="donut-images"
              cx="50" cy="50" :r="R"
              :stroke-dasharray="`${donut.images} ${donut.C}`"
              stroke-dashoffset="0"
              transform="rotate(-90 50 50)"
            />
            <!-- Videos arc -->
            <circle
              v-if="donut.videos > 0"
              class="donut-videos"
              cx="50" cy="50" :r="R"
              :stroke-dasharray="`${donut.videos} ${donut.C}`"
              :stroke-dashoffset="`${-donut.images}`"
              transform="rotate(-90 50 50)"
            />
            <!-- Center label -->
            <text x="50" y="46" class="donut-count">{{ org.scanResult.total }}</text>
            <text x="50" y="58" class="donut-label">files</text>
          </svg>
        </div>
        <div class="donut-legend">
          <div class="legend-group">
            <div class="legend-row">
              <span class="legend-dot legend-dot--images" />
              <span class="legend-name">Images</span>
              <span class="legend-value">{{ org.scanResult.images }}</span>
              <span class="legend-pct">{{ donut.imagesPct }}%</span>
            </div>
            <div class="ext-list">
              <span
                v-for="(count, ext) in org.scanResult.imageExts"
                :key="ext"
                class="ext-pill ext-pill--images"
              >.{{ ext }} <em>{{ count }}</em></span>
            </div>
          </div>
          <div class="legend-group">
            <div class="legend-row">
              <span class="legend-dot legend-dot--videos" />
              <span class="legend-name">Videos</span>
              <span class="legend-value">{{ org.scanResult.videos }}</span>
              <span class="legend-pct">{{ donut.videosPct }}%</span>
            </div>
            <div class="ext-list">
              <span
                v-for="(count, ext) in org.scanResult.videoExts"
                :key="ext"
                class="ext-pill ext-pill--videos"
              >.{{ ext }} <em>{{ count }}</em></span>
            </div>
          </div>
        </div>
      </div>

      <!-- Action buttons -->
      <div v-if="org.scanResult" class="action-bar">
        <button class="btn btn-secondary" :disabled="isBusy || !org.folders.length" @click="org.runPreviewRewrite()">
          Preview rewrite date
        </button>
        <button class="btn btn-success" :disabled="isBusy || !org.folders.length || !hasDatePreview" @click="askConfirm('rewriteDate')">
          Run rewrite date
        </button>
        <button class="btn btn-secondary" :disabled="isBusy || !org.folders.length || needsOutputDir" @click="org.runPreview()">
          {{ org.config.onlyRename ? 'Preview rename' : 'Preview rename &amp; move' }}
        </button>
        <button class="btn btn-success" :disabled="isBusy || !org.folders.length || needsOutputDir || !hasRenamePreview" @click="askConfirm('rename')">
          {{ org.config.onlyRename ? 'Run rename' : 'Run rename &amp; move' }}
        </button>
      </div>

      <!-- Scan progress (shown before first results arrive) -->
      <ScanProgress
        v-if="org.scanning && !sortedFiles.length"
        :title="busyTitle"
      />

      <!-- File list table + operation progress overlay -->
      <div v-if="sortedFiles.length" class="table-area">
        <ScanProgress
          v-if="isBusy"
          :title="busyTitle"
          :progress="{ scanned: org.progress.processed, total: org.progress.total }"
          :progress-percent="orgProgressPercent"
        />
        <div class="file-table-wrap">
        <table class="file-table">
          <thead>
            <tr>
              <th class="col-icon" />
              <th class="col-name">Name</th>
              <th v-if="hasRenamePreview" class="col-new-name">New name</th>
              <th class="col-path">Path</th>
              <th v-if="hasRenamePreview && !org.config.onlyRename" class="col-new-path">New path</th>
              <th :class="['col-date', { 'col-shrink': hasDatePreview }]">Date taken</th>
              <th v-if="hasDatePreview" class="col-new-date">New date taken</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="f in sortedFiles" :key="f.path" class="file-row">
              <td class="file-type-icon">{{ fileType(f.name) === 'video' ? '🎥' : '🖼️' }}</td>
              <td class="file-name" :title="f.name">{{ f.name }}</td>
              <td v-if="hasRenamePreview" class="file-new-name" :title="previewByPath.get(normPath(f.path))?.newFilename">
                {{ previewByPath.get(normPath(f.path))?.newFilename ?? '—' }}
              </td>
              <td class="file-path" :title="f.path">{{ f.path }}</td>
              <td v-if="hasRenamePreview && !org.config.onlyRename" class="file-new-path" :title="previewByPath.get(normPath(f.path))?.newPath">
                {{ previewByPath.get(normPath(f.path))?.newPath ?? '—' }}
              </td>
              <td class="file-date">
                <span class="file-date-inner">
                  {{ formatDate(f.dateTaken) }}
                  <span v-if="f.dateSource === 'exif'" class="date-source-badge date-source-badge--exif" title="Date from DateTimeOriginal (EXIF)">EX</span>
                  <span v-if="f.dateSource === 'filename'" class="date-source-badge date-source-badge--filename" title="Date extracted from filename">NA</span>
                  <span v-if="f.dateSource === 'create'" class="date-source-badge date-source-badge--create" title="Date from CreateDate, not DateTimeOriginal">CD</span>
                  <span v-if="f.dateSource === 'modify'" class="date-source-badge date-source-badge--modify" title="Date from FileModifyDate (no EXIF date found)">MD</span>
                </span>
              </td>
              <td v-if="hasDatePreview" class="file-new-date">
                {{ formatDate(previewDateByPath.get(normPath(f.path))?.date) }}
              </td>
            </tr>
          </tbody>
        </table>
        </div><!-- /file-table-wrap -->
      </div><!-- /table-area -->



    </template>

    <!-- Result modal -->
    <Teleport to="body">
      <div v-if="org.lastSummary" class="modal-backdrop" @click.self="org.lastSummary = null">
        <div class="modal-box">
          <div class="modal-header">
            <p class="modal-op">Operation complete</p>
            <p class="modal-title" :class="org.lastSummary.failed ? 'modal-title--warn' : 'modal-title--ok'">
              {{ org.lastSummary.failed ? `${org.lastSummary.failed} file(s) failed` : 'All files processed successfully' }}
            </p>
          </div>
          <div class="result-stats">
            <span class="result-stat result-ok">✓ {{ org.lastSummary.succeeded }} succeeded</span>
            <span v-if="org.lastSummary.failed" class="result-stat result-fail">✗ {{ org.lastSummary.failed }} failed</span>
            <span class="result-stat result-total">{{ org.lastSummary.total }} total</span>
          </div>
          <div v-if="org.lastSummary.failedPaths?.length" class="failed-list">
            <p class="failed-list-label">Failed files</p>
            <div
              v-for="p in org.lastSummary.failedPaths"
              :key="p"
              class="failed-item"
              :title="p"
            >{{ shortPath(p) }}</div>
          </div>
          <div class="modal-footer">
            <button class="btn btn-primary" @click="org.lastSummary = null">Close</button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- Confirmation modal -->
    <Teleport to="body">
      <div v-if="confirmAction" class="modal-backdrop" @click.self="confirmAction = null">
        <div class="modal-box">
          <div class="modal-header">
            <p class="modal-op">
              {{ confirmAction === 'rewriteDate' ? 'Run rewrite date' : (org.config.onlyRename ? 'Run rename' : 'Run rename & move') }}
            </p>
            <p class="modal-title">Irreversible action</p>
          </div>
          <p class="modal-body">
            This operation will permanently modify files on disk and cannot be undone.
            Are you sure you want to continue?
          </p>
          <div class="modal-footer">
            <button class="btn btn-secondary" @click="confirmAction = null">Cancel</button>
            <button class="btn btn-success" @click="runConfirmed">Confirm</button>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<script setup>
import { computed, ref } from 'vue'
import { useOrganizerStore } from '../store/organizer'
import ScanProgress from './ScanProgress.vue'

const org = useOrganizerStore()

const R = 36
const C = 2 * Math.PI * R

const donut = computed(() => {
  const { total, images, videos } = org.scanResult ?? { total: 0, images: 0, videos: 0 }
  const imgArc = total > 0 ? (images / total) * C : 0
  const vidArc = total > 0 ? (videos / total) * C : 0
  return {
    R, C,
    images: imgArc,
    videos: vidArc,
    imagesPct: total > 0 ? Math.round(images / total * 100) : 0,
    videosPct: total > 0 ? Math.round(videos / total * 100) : 0,
  }
})

const confirmAction = ref(null) // 'rewriteDate' | 'rename' | null

function askConfirm(action) { confirmAction.value = action }

function runConfirmed() {
  const action = confirmAction.value
  confirmAction.value = null
  if (action === 'rewriteDate') org.runMetadataRewrite()
  else if (action === 'rename') org.runExecute()
}

function normPath(p) { return p.replace(/\\/g, '/') }

const hasRenamePreview = computed(() => org.previewActions.length > 0)
const hasDatePreview   = computed(() => org.previewDateActions.length > 0)

const previewByPath = computed(() => {
  const map = new Map()
  for (const a of org.previewActions) map.set(normPath(a.originalPath), a)
  return map
})

const previewDateByPath = computed(() => {
  const map = new Map()
  for (const a of org.previewDateActions) map.set(normPath(a.path), a)
  return map
})

const VIDEO_EXTS = new Set(['mp4', 'mov', 'avi', 'mpg', 'mpeg', 'mkv'])

function fileType(name) {
  const ext = name.split('.').pop()?.toLowerCase() ?? ''
  return VIDEO_EXTS.has(ext) ? 'video' : 'image'
}

const sortedFiles = computed(() => {
  const files = org.scanResult?.files ?? []
  const dir = org.sortDir === 'asc' ? 1 : -1
  return [...files].sort((a, b) => {
    if (org.sortBy === 'type') {
      const typeA = fileType(a.name)
      const typeB = fileType(b.name)
      if (typeA !== typeB) return typeA.localeCompare(typeB) * dir
    }
    if (org.sortBy === 'date') {
      const da = a.dateTaken ?? ''
      const db = b.dateTaken ?? ''
      if (da !== db) return da.localeCompare(db) * dir
    }
    return a.name.toLowerCase().localeCompare(b.name.toLowerCase()) * dir
  })
})

const orgProgressPercent = computed(() => {
  const { processed, total } = org.progress
  if (!total) return 0
  return Math.round((processed / total) * 100)
})

const needsOutputDir = computed(() =>
  !org.config.onlyRename && !org.config.outputDirectory
)

const isBusy = computed(() =>
  org.scanning || org.previewing || org.previewingDate || org.executing
)

const busyTitle = computed(() => {
  if (org.scanning)       return 'Scanning…'
  if (org.previewingDate) return 'Previewing date rewrite…'
  if (org.previewing)     return org.config.onlyRename ? 'Previewing rename…' : 'Previewing rename & move…'
  if (org.executing && org.executingOp === 'rewrite') return 'Rewriting dates…'
  if (org.executing)      return org.config.onlyRename ? 'Renaming…' : 'Renaming & moving…'
  return 'Processing…'
})


function formatDate(d) {
  // ExifTool format: "YYYY:MM:DD HH:MM:SS" (FileModifyDate may have "+HH:MM" suffix)
  if (!d || d.length < 10) return d ?? ''
  const clean = d.slice(0, 19)  // strip timezone suffix
  const [datePart, timePart] = clean.split(' ')
  if (!datePart) return d
  const [y, m, day] = datePart.split(':')
  const date = `${day}-${m}-${y}`
  return timePart ? `${date} ${timePart}` : date
}

</script>

<style scoped>
.organizer-view {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  padding: var(--space-3);
  gap: var(--space-3);
}

/* ── Empty state ── */
.empty-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  color: var(--text-muted);
}
.empty-title { font-size: var(--font-size-lg); font-weight: 500; }
.empty-sub   { font-size: var(--font-size-sm); }

/* ── Error ── */
.error-banner {
  padding: var(--space-2) var(--space-3);
  background: var(--color-danger);
  color: #fff;
  border-radius: var(--border-radius-sm);
  font-size: var(--font-size-sm);
}

/* ── Scan result card ── */
.scan-result-card {
  display: flex;
  align-items: center;
  gap: 24px;
  padding: 20px 24px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-md);
}

/* Donut */
.donut-wrap { flex-shrink: 0; width: 140px; height: 140px; }
.donut-svg  { width: 100%; height: 100%; }

.donut-track {
  fill: none;
  stroke: var(--border-color);
  stroke-width: 14;
}
.donut-images {
  fill: none;
  stroke: var(--color-accent);
  stroke-width: 14;
  stroke-linecap: butt;
  transition: stroke-dasharray 0.4s ease;
}
.donut-videos {
  fill: none;
  stroke: var(--color-success);
  stroke-width: 14;
  stroke-linecap: butt;
  transition: stroke-dasharray 0.4s ease;
}
.donut-count {
  fill: var(--text-primary);
  font-size: 22px;
  font-weight: 700;
  text-anchor: middle;
  dominant-baseline: middle;
}
.donut-label {
  fill: var(--text-muted);
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.8px;
  text-anchor: middle;
  dominant-baseline: middle;
}

/* Legend */
.donut-legend {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-width: 0;
}
.legend-group {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.legend-row {
  display: grid;
  grid-template-columns: 10px 1fr auto auto;
  align-items: center;
  gap: 10px;
}
.legend-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  flex-shrink: 0;
}
.legend-dot--images { background: var(--color-accent); }
.legend-dot--videos { background: var(--color-success); }

.legend-name {
  font-size: 13px;
  color: var(--text-secondary);
}
.legend-value {
  font-size: 16px;
  font-weight: 700;
  color: var(--text-primary);
  text-align: right;
}
.legend-pct {
  font-size: 11px;
  color: var(--text-muted);
  min-width: 36px;
  text-align: right;
}

/* Extension pills */
.ext-list {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  padding-left: 20px;
}
.ext-pill {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 7px;
  border-radius: var(--border-radius-pill);
  font-size: 10px;
  font-weight: 500;
  letter-spacing: 0.3px;
}
.ext-pill em {
  font-style: normal;
  font-weight: 700;
  opacity: 0.8;
}
.ext-pill--images {
  background: color-mix(in srgb, var(--color-accent) 15%, transparent);
  color: var(--color-accent);
}
.ext-pill--videos {
  background: color-mix(in srgb, var(--color-success) 15%, transparent);
  color: var(--color-success);
}

/* ── Action bar ── */
.action-bar {
  display: flex;
  gap: var(--space-2);
  flex-wrap: wrap;
}
.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 7px var(--space-3);
  border-radius: var(--border-radius-md);
  font-size: var(--font-size-sm);
  font-weight: 500;
  white-space: nowrap;
  transition: background var(--transition), opacity var(--transition);
  cursor: pointer;
}
.btn:disabled { opacity: 0.35; cursor: not-allowed; }
.btn-full { width: 100%; }
.btn-primary  { background: var(--color-accent);   color: #fff; }
.btn-primary:hover:not(:disabled)  { filter: brightness(1.12); }
.btn-success  { background: var(--color-success);  color: #fff; }
.btn-success:hover:not(:disabled)  { background: var(--color-success-hover); }
.btn-secondary { background: var(--bg-card); color: var(--text-secondary); border: 1px solid var(--border-color); }
.btn-secondary:hover:not(:disabled) { background: var(--bg-card-hover); }
.btn-danger   { background: var(--color-danger);   color: #fff; }
.btn-danger:hover:not(:disabled)   { background: var(--color-danger-hover); }

/* ── Table area (table + progress overlay) ── */
.table-area {
  flex: 1;
  min-height: 0;
  position: relative;
  display: flex;
  flex-direction: column;
}
.table-area :deep(.scan-overlay) {
  position: absolute;
  inset: 0;
  z-index: 10;
  background: color-mix(in srgb, var(--bg-primary) 80%, transparent);
  backdrop-filter: blur(2px);
}

/* ── File table ── */
.file-table-wrap {
  flex: 1;
  overflow-y: auto;
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-md);
}
.file-table {
  border-collapse: collapse;
  width: 100%;
  table-layout: auto;
}

.file-table thead {
  position: sticky;
  top: 0;
  z-index: 1;
  background: var(--bg-card);
}
.file-table th {
  padding: 6px 8px;
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.6px;
  color: var(--text-muted);
  text-align: left;
  border-bottom: 1px solid var(--border-color);
  white-space: nowrap;
}
.col-icon, .col-name, .col-path, .col-new-name, .col-new-path { width: 1px; }
.col-shrink { width: 1px; }
.col-icon { padding-right: 4px; }

.file-row { transition: background var(--transition); }
.file-row:last-child td { border-bottom: none; }
.file-row:hover { background: var(--bg-card-hover); }
.file-row td {
  padding: 5px 8px;
  border-bottom: 1px solid var(--border-color);
  white-space: nowrap;
}

.file-type-icon { font-size: 13px; line-height: 1; padding-right: 4px; }

.file-name {
  font-size: 12px;
  color: var(--text-primary);
  font-weight: 500;
  white-space: nowrap;
}
.file-path {
  font-size: 11px;
  color: var(--text-muted);
  white-space: nowrap;
}
.file-new-date {
  font-size: 12px;
  font-weight: 500;
  color: var(--color-accent);
  white-space: nowrap;
}
.file-new-name {
  font-size: 12px;
  font-weight: 500;
  color: var(--color-accent);
  white-space: nowrap;
}
.file-new-path {
  font-size: 12px;
  font-weight: 500;
  color: var(--color-accent);
  white-space: nowrap;
}
.file-date-inner {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
  color: var(--text-secondary);
  white-space: nowrap;
}
.date-source-badge {
  font-size: 9px;
  font-weight: 700;
  padding: 1px 4px;
  border-radius: 3px;
  letter-spacing: 0.3px;
  flex-shrink: 0;
}
.date-source-badge--exif     { background: #1a3a22; color: #4caf7d; }
.date-source-badge--filename { background: #2a1a3a; color: #b07adf; }
.date-source-badge--create   { background: #3a2a1a; color: #d4853a; }
.date-source-badge--modify   { background: #1e2a3a; color: #7aabcf; }


/* ── Failed list ── */
.failed-list {
  display: flex;
  flex-direction: column;
  gap: 3px;
  max-height: 120px;
  overflow-y: auto;
}
.failed-list-label {
  font-size: var(--font-size-xs);
  color: var(--color-danger);
  text-transform: uppercase;
  letter-spacing: 0.6px;
}
.failed-item {
  font-size: 11px;
  color: var(--text-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* ── Confirmation modal ── */
.modal-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 9999;
}
.modal-box {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-md);
  padding: var(--space-4);
  max-width: 360px;
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}
.modal-header {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding-bottom: var(--space-2);
  border-bottom: 1px solid var(--border-color);
}
.modal-op {
  font-size: var(--font-size-sm);
  font-weight: 700;
  color: var(--text-primary);
}
.modal-title {
  font-size: var(--font-size-xs);
  font-weight: 500;
  color: var(--color-danger);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}
.modal-title--ok   { color: var(--color-success); }
.modal-title--warn { color: var(--color-danger); }
.result-stats {
  display: flex;
  gap: var(--space-3);
  flex-wrap: wrap;
}
.result-stat       { font-size: var(--font-size-sm); font-weight: 600; }
.result-ok         { color: var(--color-success); }
.result-fail       { color: var(--color-danger); }
.result-total      { color: var(--text-muted); font-weight: 400; }
.modal-body {
  font-size: var(--font-size-sm);
  color: var(--text-secondary);
  line-height: 1.5;
}
.modal-footer {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2);
}
</style>
