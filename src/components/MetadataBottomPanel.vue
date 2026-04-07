<template>
  <Transition name="mbp-slide">
    <div
      v-if="store.metadataPanel && activeMode === 'metadata'"
      class="mbp-panel"
      :style="{ height: panelHeight + 'px' }"
      @keydown.escape="store.closeMetadataPanel()"
    >
      <!-- Resize handle (top edge) -->
      <div class="mbp-resize-handle" @mousedown.prevent="startResize" />

      <!-- Header row -->
      <div class="mbp-header">
        <!-- Left: thumbnail + file info -->
        <div class="mbp-left">
          <div class="mbp-thumb-wrap">
            <img
              v-if="thumbSrc"
              :src="thumbSrc"
              class="mbp-thumb"
              draggable="false"
            />
            <div v-else class="mbp-thumb-placeholder">
              <span class="mbp-thumb-ext">{{ fileExt(entry.path).toUpperCase() }}</span>
            </div>
          </div>
          <div class="mbp-file-info">
            <p class="mbp-filename" :title="entry.path">{{ fileName(entry.path) }}</p>
            <p class="mbp-filepath" :title="entry.path">{{ folderPath(entry.path) }}</p>
          </div>
        </div>

        <button class="mbp-close" @click="store.closeMetadataPanel()" title="Close">✕</button>
      </div>

      <!-- Loading state -->
      <div v-if="panel.loading" class="mbp-loading">
        <span class="mbp-spinner" />
        <span>Reading metadata…</span>
      </div>

      <!-- Error state -->
      <div v-else-if="panel.error" class="mbp-error">
        {{ panel.error }}
      </div>

      <!-- Horizontal sections -->
      <div v-else-if="meta" class="mbp-sections-wrap">
        <div class="mbp-sections">

          <!-- File + Camera -->
          <div class="mbp-section">
            <div class="mbp-section-title">File &amp; Camera</div>
            <div class="mbp-rows">
              <div class="mbp-row"><span class="mbp-label">Size</span><span class="mbp-value">{{ formatSize(meta.fileSize) }}</span></div>
              <div class="mbp-row" v-if="meta.width > 0"><span class="mbp-label">Dims</span><span class="mbp-value">{{ meta.width }}×{{ meta.height }}</span></div>
              <template v-if="hasCameraInfo">
                <div class="mbp-row" v-if="meta.make || meta.model">
                  <span class="mbp-label">Device</span>
                  <span class="mbp-value">{{ [meta.make, meta.model].filter(Boolean).join(' ') }}</span>
                </div>
                <div class="mbp-row" v-if="meta.lensModel">
                  <span class="mbp-label">Lens</span>
                  <span class="mbp-value">{{ meta.lensModel }}</span>
                </div>
                <div class="mbp-row" v-if="meta.software">
                  <span class="mbp-label">Software</span>
                  <span class="mbp-value">{{ meta.software }}</span>
                </div>
              </template>
            </div>
          </div>

          <!-- Date -->
          <div class="mbp-section">
            <div class="mbp-section-title">Date taken</div>
            <div class="mbp-edit-rows">
              <label class="mbp-edit-row">
                <input
                  class="mbp-input"
                  type="datetime-local"
                  step="1"
                  :value="isoToDatetimeLocal(edit.dateTimeOriginal)"
                  @change="e => { edit.dateTimeOriginal = datetimeLocalToIso(e.target.value); panel.dirty = true }"
                />
              </label>
            </div>
          </div>

          <!-- Location -->
          <div class="mbp-section mbp-section--location">
            <!-- Left: title + geocoded name + GPS inputs -->
            <div class="mbp-location-left">
              <div class="mbp-section-title">Location</div>
              <div class="mbp-rows" v-if="hasGpsPreview">
                <div class="mbp-row" v-if="locationLoading">
                  <span class="mbp-label">Location</span>
                  <span class="mbp-value mbp-value--muted">Fetching…</span>
                </div>
                <div class="mbp-row" v-else-if="locationName">
                  <span class="mbp-label">Location</span>
                  <span class="mbp-value">{{ locationName }}</span>
                </div>
              </div>
              <!-- Combined input -->
              <div v-if="showCombinedInput" class="mbp-edit-rows">
                <label class="mbp-edit-row">
                  <input
                    class="mbp-input"
                    type="text"
                    v-model="gpsCombinedRaw"
                    @input="onCombinedInput"
                    :class="{ 'mbp-input--error': gpsCombinedError }"
                    placeholder="39°48'43.1&quot;N 0°25'29.1&quot;W"
                  />
                </label>
                <p v-if="gpsCombinedError" class="mbp-gps-error">{{ gpsCombinedError }}</p>
              </div>
              <!-- Split inputs -->
              <div v-else class="mbp-gps-col">
                <label class="mbp-edit-row">
                  <span class="mbp-label">Latitude</span>
                  <input
                    class="mbp-input"
                    type="text"
                    v-model="gpsLatitudeRaw"
                    @input="onGpsInput('lat')"
                    @blur="normalizeGpsInput('lat')"
                    :class="{ 'mbp-input--error': gpsLatError }"
                    placeholder="40.71600"
                  />
                </label>
                <label class="mbp-edit-row">
                  <span class="mbp-label">Longitude</span>
                  <input
                    class="mbp-input"
                    type="text"
                    v-model="gpsLongitudeRaw"
                    @input="onGpsInput('lon')"
                    @blur="normalizeGpsInput('lon')"
                    :class="{ 'mbp-input--error': gpsLonError }"
                    placeholder="-74.00600"
                  />
                </label>
                <p v-if="gpsLatError || gpsLonError" class="mbp-gps-error">
                  {{ gpsLatError || gpsLonError }}
                </p>
              </div>
            </div>
            <!-- Right: map filling full height -->
            <div v-if="hasGpsPreview" class="mbp-location-map">
              <MapPreview :lat="previewLat" :lon="previewLon" :scroll-wheel-zoom="true" />
            </div>
          </div>

          <!-- Exposure -->
          <div class="mbp-section" v-if="hasExposureInfo">
            <div class="mbp-section-title">Exposure</div>
            <div class="mbp-rows">
              <div class="mbp-row" v-if="meta.exposureTime"><span class="mbp-label">Shutter</span><span class="mbp-value">{{ meta.exposureTime }}</span></div>
              <div class="mbp-row" v-if="meta.fNumber"><span class="mbp-label">Aperture</span><span class="mbp-value">{{ meta.fNumber }}</span></div>
              <div class="mbp-row" v-if="meta.isoSpeed"><span class="mbp-label">ISO</span><span class="mbp-value">{{ meta.isoSpeed }}</span></div>
              <div class="mbp-row" v-if="meta.focalLength"><span class="mbp-label">Focal length</span><span class="mbp-value">{{ meta.focalLength }}</span></div>
              <div class="mbp-row" v-if="meta.flash"><span class="mbp-label">Flash</span><span class="mbp-value">{{ meta.flash }}</span></div>
              <div class="mbp-row" v-if="meta.whiteBalance"><span class="mbp-label">White balance</span><span class="mbp-value">{{ meta.whiteBalance }}</span></div>
              <div class="mbp-row" v-if="meta.exposureMode"><span class="mbp-label">Exp. mode</span><span class="mbp-value">{{ meta.exposureMode }}</span></div>
              <div class="mbp-row" v-if="meta.meteringMode"><span class="mbp-label">Metering</span><span class="mbp-value">{{ meta.meteringMode }}</span></div>
            </div>
          </div>

          <!-- Details (editable) -->
          <div class="mbp-section mbp-section--details">
            <div class="mbp-section-title">Details</div>
            <div class="mbp-edit-rows">
              <label class="mbp-edit-row">
                <span class="mbp-label">Description</span>
                <input
                  class="mbp-input"
                  type="text"
                  v-model="edit.imageDescription"
                  @input="panel.dirty = true"
                  placeholder="Add a description…"
                />
              </label>
              <label class="mbp-edit-row">
                <span class="mbp-label">Artist</span>
                <input
                  class="mbp-input"
                  type="text"
                  v-model="edit.artist"
                  @input="panel.dirty = true"
                  placeholder="Photographer name…"
                />
              </label>
              <label class="mbp-edit-row">
                <span class="mbp-label">Copyright</span>
                <input
                  class="mbp-input"
                  type="text"
                  v-model="edit.copyright"
                  @input="panel.dirty = true"
                  placeholder="© Year Name…"
                />
              </label>
            </div>
            <p class="mbp-save-notice" v-if="!panel.dirty && !panel.saving && !saveError">
              Changes are written directly to the file's EXIF data.
            </p>
          </div>

        </div>

        <!-- Floating action bar -->
        <Transition name="mbp-bar">
          <div class="mbp-float-bar" v-if="panel.dirty || panel.saving">
            <p v-if="saveError" class="mbp-save-error">{{ saveError }}</p>
            <div class="mbp-actions">
              <button class="mbp-btn mbp-btn-ghost" @click="resetEdit" :disabled="panel.saving">Reset</button>
              <button class="mbp-btn mbp-btn-primary" @click="save" :disabled="panel.saving">
                <span v-if="panel.saving">Saving…</span>
                <span v-else>Save changes</span>
              </button>
            </div>
          </div>
        </Transition>
      </div>

    </div>
  </Transition>
</template>

<script setup>
import { ref, computed, watch, onBeforeUnmount } from 'vue'
import { convertFileSrc } from '@tauri-apps/api/core'
import { useScanStore } from '../store/scan'
import { useMode } from '../composables/useMode'
import MapPreview from './MapPreview.vue'
import { fileExt, fileName, folderPath, formatSize, isoToDatetimeLocal, datetimeLocalToIso } from '../utils/formatters'
import { useGpsEditor } from '../composables/useGpsEditor'

const store = useScanStore()
const { activeMode } = useMode()
const HEIC = new Set(['heic', 'heif'])

// ── Panel height resize ───────────────────────────────────────────────────────
const MIN_HEIGHT = 200
const panelHeight = ref(300)

function startResize(e) {
  const startY = e.clientY
  const startH = panelHeight.value

  function onMove(e) {
    const delta = startY - e.clientY
    panelHeight.value = Math.max(MIN_HEIGHT, startH + delta)
  }
  function onUp() {
    window.removeEventListener('mousemove', onMove)
    window.removeEventListener('mouseup', onUp)
  }
  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
}

onBeforeUnmount(() => {
  panelHeight.value = 300
})

// ── Store accessors ───────────────────────────────────────────────────────────
const panel = computed(() => store.metadataPanel)
const entry = computed(() => panel.value?.entry ?? {})
const meta  = computed(() => panel.value?.metadata ?? null)

const saveError = ref(null)

// ── GPS editor ────────────────────────────────────────────────────────────────
const {
  gpsLatitudeRaw, gpsLongitudeRaw,
  gpsLatError, gpsLonError,
  gpsCombinedRaw, gpsCombinedError,
  locationName, locationLoading,
  showCombinedInput,
  previewLat, previewLon, hasGpsPreview,
  onCombinedInput, onGpsInput, normalizeGpsInput,
  resetGps, validateGps,
} = useGpsEditor(meta, () => { panel.value.dirty = true })

// ── Editable fields ───────────────────────────────────────────────────────────
const edit = ref({
  dateTimeOriginal: null,
  imageDescription: '',
  artist: '',
  copyright: '',
})

function resetEdit() {
  if (!meta.value) return
  edit.value = {
    dateTimeOriginal: meta.value.dateTimeOriginal ?? null,
    imageDescription: meta.value.imageDescription ?? '',
    artist:           meta.value.artist ?? '',
    copyright:        meta.value.copyright ?? '',
  }
  resetGps(meta.value)
  if (panel.value) panel.value.dirty = false
  saveError.value = null
}

watch(meta, (m) => { if (m) resetEdit() }, { immediate: true })

async function save() {
  saveError.value = null
  const { ok, lat, lon } = validateGps()
  if (!ok) return

  await store.saveMetadata({
    dateTimeOriginal: edit.value.dateTimeOriginal || null,
    imageDescription: edit.value.imageDescription || null,
    artist:           edit.value.artist || null,
    copyright:        edit.value.copyright || null,
    gpsLatitude:      lat,
    gpsLongitude:     lon,
  })
  if (panel.value?.error) saveError.value = panel.value.error
}

// ── Thumbnail source ──────────────────────────────────────────────────────────
const thumbSrc = computed(() => {
  const p = entry.value?.path
  if (!p) return null
  const ext = fileExt(p)
  if (HEIC.has(ext)) {
    return store.thumbCache[p] && store.thumbCache[p] !== '__error__'
      ? store.thumbCache[p]
      : null
  }
  return store.directSrcCache[p] ?? convertFileSrc(p)
})

const hasCameraInfo   = computed(() => meta.value && (meta.value.make || meta.value.model || meta.value.lensModel || meta.value.software))
const hasExposureInfo = computed(() => meta.value && (meta.value.exposureTime || meta.value.fNumber || meta.value.isoSpeed || meta.value.focalLength))
</script>

<style scoped>
/* ── Panel ── */
.mbp-panel {
  position: fixed;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 150;
  background: var(--bg-secondary);
  border-top: 1px solid var(--border-color);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  box-shadow: 0 -8px 32px rgba(0, 0, 0, 0.35);
}

/* ── Resize handle (top edge) ── */
.mbp-resize-handle {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 5px;
  cursor: row-resize;
  z-index: 10;
  transition: background var(--transition);
}
.mbp-resize-handle:hover,
.mbp-resize-handle:active {
  background: var(--color-accent);
  opacity: 0.5;
}

/* ── Slide transition ── */
.mbp-slide-enter-active,
.mbp-slide-leave-active {
  transition: transform 200ms ease, opacity 200ms ease;
}
.mbp-slide-enter-from,
.mbp-slide-leave-to {
  transform: translateY(100%);
  opacity: 0;
}

/* ── Header ── */
.mbp-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 var(--space-3);
  height: 52px;
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
  gap: var(--space-3);
}

.mbp-left {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
  flex: 0 0 auto;
  width: 180px;
}

.mbp-thumb-wrap {
  width: 36px;
  height: 36px;
  flex-shrink: 0;
  border-radius: var(--border-radius-sm);
  overflow: hidden;
  background: #111;
}

.mbp-thumb {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}

.mbp-thumb-placeholder {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #1a1a1a;
}

.mbp-thumb-ext {
  font-size: 8px;
  font-weight: 700;
  color: var(--text-muted);
}

.mbp-file-info {
  min-width: 0;
  flex: 1;
}

.mbp-filename {
  font-size: var(--font-size-xs);
  font-weight: 600;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.mbp-filepath {
  font-size: 10px;
  color: var(--text-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  margin-top: 2px;
}

.mbp-close {
  width: 28px;
  height: 28px;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.06);
  color: var(--text-muted);
  font-size: 13px;
  line-height: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  padding-bottom: 1px;
  flex-shrink: 0;
  transition: background var(--transition), color var(--transition);
}
.mbp-close:hover {
  background: rgba(255, 255, 255, 0.12);
  color: var(--text-primary);
}

/* ── Loading / error ── */
.mbp-loading {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  color: var(--text-muted);
  font-size: var(--font-size-xs);
}

@keyframes spin { to { transform: rotate(360deg); } }
.mbp-spinner {
  width: 18px;
  height: 18px;
  border: 2px solid rgba(255,255,255,0.1);
  border-top-color: var(--color-accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
  flex-shrink: 0;
}

.mbp-error {
  padding: var(--space-3) var(--space-4);
  color: var(--color-danger);
  font-size: var(--font-size-xs);
}

/* ── Sections wrapper ── */
.mbp-sections-wrap {
  flex: 1;
  overflow: hidden;
  position: relative;
  display: flex;
  flex-direction: column;
}

.mbp-sections {
  flex: 1;
  display: flex;
  flex-direction: row;
  gap: 0;
  overflow-x: auto;
  overflow-y: auto;
  scrollbar-width: thin;
  scrollbar-color: rgba(255,255,255,0.1) transparent;
}

/* ── Individual section column ── */
.mbp-section {
  flex: 0 0 220px;
  min-width: 180px;
  padding: var(--space-2) var(--space-3);
  border-right: 1px solid var(--border-color);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  overflow-y: auto;
  scrollbar-width: thin;
  scrollbar-color: rgba(255,255,255,0.1) transparent;
}
.mbp-section:last-child {
  border-right: none;
  flex: 1 0 220px;
}

/* Location section — wide, row layout (inputs left + map right) */
.mbp-section--location {
  flex: 1 0 420px;
  flex-direction: row;
  padding: 0;
  gap: 0;
  overflow: hidden;
}

/* Left sub-column: title + name + GPS inputs */
.mbp-location-left {
  width: 210px;
  flex-shrink: 0;
  padding: var(--space-2) var(--space-3);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  overflow-y: auto;
  border-right: 1px solid var(--border-color);
  scrollbar-width: thin;
  scrollbar-color: rgba(255,255,255,0.1) transparent;
}

/* Right sub-column: map fills all remaining space */
.mbp-location-map {
  flex: 1;
  min-width: 0;
  height: 100%;
  overflow: hidden;
}

/* Force MapPreview root to fill the available height */
.mbp-location-map :deep(.map-wrap) {
  height: 100% !important;
  margin-top: 0;
  border-radius: 0;
  border-top: none;
  border-bottom: none;
  border-right: none;
}

/* Details section — narrower, fixed width */
.mbp-section--details {
  flex: 0 0 200px;
  min-width: 160px;
}
.mbp-section--details:last-child {
  flex: 0 0 200px;
}

.mbp-section-title {
  font-size: 10px;
  font-weight: 700;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.6px;
  flex-shrink: 0;
  white-space: nowrap;
}

/* ── Read-only rows ── */
.mbp-rows {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.mbp-row {
  display: flex;
  align-items: baseline;
  gap: var(--space-2);
  font-size: var(--font-size-xs);
}

.mbp-label {
  color: var(--text-muted);
  flex-shrink: 0;
  width: 72px;
  font-size: var(--font-size-xs);
}

.mbp-value {
  color: var(--text-secondary);
  word-break: break-word;
  font-size: var(--font-size-xs);
}
.mbp-value--muted {
  color: var(--text-muted);
  font-style: italic;
}

/* ── Editable rows ── */
.mbp-edit-rows {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.mbp-edit-row {
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.mbp-edit-row .mbp-label {
  width: auto;
  font-size: 10px;
}

.mbp-input {
  width: 100%;
  padding: 4px 7px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-primary);
  font-size: var(--font-size-xs);
  outline: none;
  transition: border-color var(--transition);
  box-sizing: border-box;
}
.mbp-input:focus { border-color: var(--color-accent); }
.mbp-input::placeholder { color: var(--text-muted); }
.mbp-input--error { border-color: var(--color-danger) !important; }

/* datetime-local color fix */
.mbp-input[type="datetime-local"]::-webkit-calendar-picker-indicator {
  filter: invert(0.6);
  cursor: pointer;
}

.mbp-gps-col {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.mbp-gps-error {
  font-size: 10px;
  color: var(--color-danger);
  margin-top: 2px;
}

/* ── Floating action bar ── */
.mbp-float-bar {
  position: absolute;
  bottom: 0;
  right: 0;
  z-index: 20;
  padding: 8px var(--space-3);
  background: var(--bg-secondary);
  border-top: 1px solid var(--border-color);
  border-left: 1px solid var(--border-color);
  box-shadow: 0 -4px 16px rgba(0, 0, 0, 0.25);
  border-radius: var(--border-radius-md) 0 0 0;
}

.mbp-bar-enter-active,
.mbp-bar-leave-active {
  transition: transform 180ms ease, opacity 180ms ease;
}
.mbp-bar-enter-from,
.mbp-bar-leave-to {
  transform: translateY(100%);
  opacity: 0;
}

.mbp-actions {
  display: flex;
  gap: var(--space-2);
}

.mbp-btn {
  padding: 5px var(--space-3);
  border-radius: var(--border-radius-sm);
  font-size: var(--font-size-xs);
  font-weight: 500;
  cursor: pointer;
  white-space: nowrap;
  transition: background var(--transition), color var(--transition), opacity var(--transition);
}
.mbp-btn:disabled { opacity: 0.5; cursor: not-allowed; }

.mbp-btn-ghost {
  background: transparent;
  color: var(--text-secondary);
  border: 1px solid var(--border-color);
}
.mbp-btn-ghost:hover:not(:disabled) { background: var(--bg-card); }

.mbp-btn-primary {
  background: var(--color-accent);
  color: #fff;
  border: none;
}
.mbp-btn-primary:hover:not(:disabled) { opacity: 0.85; }

.mbp-save-error {
  margin-bottom: var(--space-2);
  font-size: 10px;
  color: var(--color-danger);
}

.mbp-save-notice {
  font-size: 10px;
  color: var(--text-muted);
  opacity: 0.7;
  margin-top: var(--space-1);
}
</style>
