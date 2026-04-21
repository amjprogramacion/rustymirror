<template>
  <Transition name="mp-slide">
    <div v-if="panel.activePanel" class="mp-panel" :style="{ width: panelWidth + 'px' }" @keydown.escape="panel.closePanel()">
      <!-- Resize handle -->
      <div class="mp-resize-handle" @mousedown.prevent="startResize" />

      <!-- Header -->
      <div class="mp-header">
        <span class="mp-title">Image Info</span>
        <button class="mp-close" @click="panel.closePanel()" title="Close">✕</button>
      </div>

      <!-- Thumbnail — resizable height -->
      <div class="mp-thumb-wrap" :style="{ height: thumbHeight + 'px' }">
        <div class="mp-thumb-resize-handle" @mousedown.prevent="startThumbResize" />
        <img
          v-if="thumbSrc"
          :src="thumbSrc"
          class="mp-thumb"
          draggable="false"
        />
        <div v-else class="mp-thumb-placeholder">
          <div v-if="thumbLoading" class="mp-thumb-spinner" />
          <span v-else class="mp-thumb-ext">{{ fileExt(entry.path).toUpperCase() }}</span>
        </div>
        <span v-if="entry.isOriginal" class="mp-original-badge">Original</span>
      </div>

      <!-- Filename + path -->
      <div class="mp-file-header">
        <p class="mp-filename" :title="entry.path">{{ fileName(entry.path) }}</p>
        <p class="mp-filepath" :title="entry.path">{{ folderPath(entry.path) }}</p>
      </div>

      <!-- Loading state -->
      <div v-if="panel.activePanel.loading" class="mp-loading">
        <span class="mp-spinner" />
        <span>Reading metadata…</span>
      </div>

      <!-- Error state -->
      <div v-else-if="panel.activePanel.error" class="mp-error">
        {{ panel.activePanel.error }}
      </div>

      <!-- Metadata content -->
      <div v-else-if="meta" class="mp-content" :style="panel.activePanel.dirty || panel.activePanel.saving ? 'padding-bottom: 64px' : ''">

        <!-- File & Camera -->
        <PanelSectionFileCamera
          class="mp-section"
          :meta="meta"
          :collapsible="true"
          :collapsed="collapsed.file"
          @toggle="toggle('file')"
        />

        <!-- Date -->
        <PanelSectionDate
          class="mp-section"
          :value="edit.dateTimeOriginal"
          :collapsible="true"
          :collapsed="collapsed.date"
          @toggle="toggle('date')"
          @change="v => { edit.dateTimeOriginal = v; panel.activePanel.dirty = true }"
        />

        <!-- Location -->
        <div class="mp-section">
          <button class="mp-section-title" @click="toggle('location')">
            Location <ChevronIcon :open="!collapsed.location" />
          </button>
          <div v-show="!collapsed.location">
            <div class="mp-rows" v-if="hasGpsPreview">
              <div class="mp-row" v-if="locationLoading">
                <span class="mp-label">Location</span>
                <span class="mp-value mp-value--muted">Fetching…</span>
              </div>
              <div class="mp-row" v-else-if="locationName">
                <span class="mp-label">Location</span>
                <span class="mp-value">{{ locationName }}</span>
              </div>
            </div>
            <!-- Combined input: only when no GPS exists yet -->
            <div v-if="showCombinedInput" class="mp-edit-rows" style="margin-bottom: 8px">
              <label class="mp-edit-row">
                <input
                  class="mp-input"
                  type="text"
                  v-model="gpsCombinedRaw"
                  @input="onCombinedInput"
                  :class="{ 'mp-input--error': gpsCombinedError }"
                  placeholder="39°48'43.1&quot;N 0°25'29.1&quot;W"
                />
              </label>
              <p v-if="gpsCombinedError" class="mp-gps-error">{{ gpsCombinedError }}</p>
            </div>
            <!-- Split inputs: when GPS already exists or has been parsed -->
            <div v-else class="mp-gps-row" style="margin-top: 10px; margin-bottom: 8px">
              <label class="mp-edit-row">
                <span class="mp-label mp-gps-label">Latitude</span>
                <input
                  class="mp-input"
                  type="text"
                  v-model="gpsLatitudeRaw"
                  @input="onGpsInput('lat')"
                  @blur="normalizeGpsInput('lat')"
                  :class="{ 'mp-input--error': gpsLatError }"
                  placeholder="40.71600 or 40°42'57.6&quot;N"
                />
              </label>
              <label class="mp-edit-row">
                <span class="mp-label mp-gps-label">Longitude</span>
                <input
                  class="mp-input"
                  type="text"
                  v-model="gpsLongitudeRaw"
                  @input="onGpsInput('lon')"
                  @blur="normalizeGpsInput('lon')"
                  :class="{ 'mp-input--error': gpsLonError }"
                  placeholder="-74.00600 or 0°25'29.1&quot;W"
                />
              </label>
              <p v-if="gpsLatError || gpsLonError" class="mp-gps-error">
                {{ gpsLatError || gpsLonError }}
              </p>
            </div>
            <MapPreview ref="mapPreviewRef" :lat="mapCenter.lat" :lon="mapCenter.lon" :show-marker="hasGpsPreview" :reset-key="mapResetKey" :saved-view="savedMapView" @set-location="onMapSetLocation" />
          </div>
        </div>

        <!-- Exposure -->
        <PanelSectionExposure
          v-if="hasExposureInfo"
          class="mp-section"
          :meta="meta"
          :collapsible="true"
          :collapsed="collapsed.exposure"
          @toggle="toggle('exposure')"
        />

        <!-- Editable fields -->
        <PanelSectionDetails
          class="mp-section mp-section--edit"
          :description="edit.imageDescription"
          :artist="edit.artist"
          :copyright="edit.copyright"
          :show-notice="!panel.activePanel.dirty && !panel.activePanel.saving"
          :collapsible="true"
          :collapsed="collapsed.details"
          @toggle="toggle('details')"
          @update:description="v => { edit.imageDescription = v; panel.activePanel.dirty = true }"
          @update:artist="v => { edit.artist = v; panel.activePanel.dirty = true }"
          @update:copyright="v => { edit.copyright = v; panel.activePanel.dirty = true }"
        />

      </div>

      <!-- Floating action bar -->
      <Transition name="mp-bar">
        <div class="mp-float-bar" v-if="panel.activePanel.dirty || panel.activePanel.saving">
          <div class="mp-actions">
            <button class="mp-btn mp-btn-ghost" @click="resetEdit" :disabled="panel.activePanel.saving">Reset</button>
            <button class="mp-btn mp-btn-primary" @click="save" :disabled="panel.activePanel.saving">
              <span v-if="panel.activePanel.saving">Saving…</span>
              <span v-else>Save changes</span>
            </button>
          </div>
        </div>
      </Transition>

    </div>
  </Transition>

  <!-- Save notification (centered) -->
  <Teleport to="body">
    <Transition name="mp-notify">
      <div v-if="notification" class="mp-notify-overlay" @click="notification = null">
        <div class="mp-notify-card" :class="`mp-notify--${notification.type}`" @click.stop>
          <span class="mp-notify-icon">{{ notification.type === 'success' ? '✓' : '✕' }}</span>
          <span class="mp-notify-msg">{{ notification.message }}</span>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup>
import { ref, computed, watch, onBeforeUnmount } from 'vue'
import { convertFileSrc } from '@tauri-apps/api/core'
import { useDuplicatesStore } from '../store/duplicates'
import { usePanelStore } from '../store/panel'
import { useThumbnailStore } from '../store/thumbnails'
import { useMapViewStore } from '../store/mapView'
import MapPreview from './MapPreview.vue'
import ChevronIcon from './ChevronIcon.vue'
import { fileExt, fileName, folderPath } from '../utils/formatters'
import { useGpsEditor } from '../composables/useGpsEditor'
import PanelSectionFileCamera from './PanelSectionFileCamera.vue'
import PanelSectionExposure   from './PanelSectionExposure.vue'
import PanelSectionDate       from './PanelSectionDate.vue'
import PanelSectionDetails    from './PanelSectionDetails.vue'
import { MP_MIN_WIDTH, MP_MIN_THUMB_HEIGHT } from '../constants'

const store  = useDuplicatesStore()
const panel  = usePanelStore()
const thumbs = useThumbnailStore()
const HEIC   = new Set(['heic', 'heif'])

const panelWidth  = ref(MP_MIN_WIDTH)
const mapViewStore  = useMapViewStore()
const mapResetKey   = ref(0)
const savedMapView  = ref(mapViewStore.duplicates)
const mapPreviewRef = ref(null)

// Save map state the moment the panel closes (before v-if removes MapPreview from DOM)
watch(() => panel.activePanel, (p, prev) => {
  if (prev && !p) {
    const state = mapPreviewRef.value?.getMapState()
    if (state) mapViewStore.duplicates = state
    savedMapView.value = null
  }
}, { flush: 'sync' })
const thumbHeight = ref(MP_MIN_THUMB_HEIGHT)

function startThumbResize(e) {
  const startY = e.clientY
  const startH = thumbHeight.value

  function onMove(e) {
    thumbHeight.value = Math.max(MP_MIN_THUMB_HEIGHT, startH + (e.clientY - startY))
  }
  function onUp() {
    window.removeEventListener('mousemove', onMove)
    window.removeEventListener('mouseup', onUp)
  }
  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
}

function startResize(e) {
  const startX = e.clientX
  const startW = panelWidth.value

  function onMove(e) {
    const delta = startX - e.clientX
    panelWidth.value = Math.max(MP_MIN_WIDTH, startW + delta)
  }
  function onUp() {
    window.removeEventListener('mousemove', onMove)
    window.removeEventListener('mouseup', onUp)
  }
  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
}

onBeforeUnmount(() => {
  panelWidth.value  = MP_MIN_WIDTH
  thumbHeight.value = MP_MIN_THUMB_HEIGHT
  const state = mapPreviewRef.value?.getMapState()
  if (state) mapViewStore.duplicates = state
})

const activePanel = computed(() => panel.activePanel)
const entry  = computed(() => activePanel.value?.entry ?? {})
const meta   = computed(() => activePanel.value?.metadata ?? null)

// ── Section collapse state ────────────────────────────────────────────────────
const collapsed = ref({ file: false, date: false, location: false, exposure: true, details: true })
function toggle(key) { collapsed.value[key] = !collapsed.value[key] }
watch(meta, (m) => {
  if (m) collapsed.value = { file: false, date: false, location: false, exposure: true, details: true }
  mapResetKey.value++
})

const notification = ref(null)
let   notifyTimer  = null

function showNotification(type, message, duration = 2500) {
  clearTimeout(notifyTimer)
  notification.value = { type, message }
  notifyTimer = setTimeout(() => { notification.value = null }, duration)
}

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
} = useGpsEditor(meta, () => { panel.activePanel.dirty = true })

const SPAIN_CENTER = { lat: 40.416775, lon: -3.703790 }
const mapCenter = computed(() => ({
  lat: previewLat.value ?? SPAIN_CENTER.lat,
  lon: previewLon.value ?? SPAIN_CENTER.lon,
}))

function onMapSetLocation({ lat, lon }) {
  gpsLatitudeRaw.value  = lat.toFixed(6)
  gpsLongitudeRaw.value = lon.toFixed(6)
  onGpsInput('lat')
  onGpsInput('lon')
}

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
  if (panel.activePanel) panel.activePanel.dirty = false
}

watch(meta, (m) => { if (m) resetEdit() }, { immediate: true })

async function save() {
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
  if (panel.activePanel?.error) {
    showNotification('error', panel.activePanel.error, 4000)
  } else {
    showNotification('success', 'Saved successfully')
  }
}

// Enqueue thumbnail generation for HEIC files opened directly via panel
watch(() => entry.value?.path, (path) => {
  if (!path) return
  if (HEIC.has(fileExt(path)) && !thumbs.thumbCache[path]) {
    thumbs.enqueueThumbnail(path)
  }
}, { immediate: true })

// ── Thumbnail source ──────────────────────────────────────────────────────────
const thumbLoading = computed(() => {
  const p = entry.value?.path
  if (!p || !HEIC.has(fileExt(p))) return false
  return !(p in thumbs.thumbCache)
})

const thumbSrc = computed(() => {
  const p = entry.value?.path
  if (!p) return null
  const ext = fileExt(p)
  if (HEIC.has(ext)) {
    return thumbs.thumbCache[p] && thumbs.thumbCache[p] !== '__error__'
      ? thumbs.thumbCache[p]
      : null
  }
  return thumbs.directSrcCache[p] ?? convertFileSrc(p)
})

const hasExposureInfo = computed(() => meta.value && (meta.value.exposureTime || meta.value.fNumber || meta.value.isoSpeed || meta.value.focalLength))
</script>

<style scoped>
.mp-panel {
  position: fixed;
  top: 44px;
  right: 0;
  bottom: 0;
  z-index: 150;
  background: var(--bg-secondary);
  border-left: 1px solid var(--border-color);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  box-shadow: -8px 0 32px rgba(0, 0, 0, 0.35);
}

/* ── Resize handle ── */
.mp-resize-handle {
  position: absolute;
  top: 0;
  left: 0;
  width: 5px;
  height: 100%;
  cursor: col-resize;
  z-index: 10;
  transition: background var(--transition);
}
.mp-resize-handle:hover,
.mp-resize-handle:active {
  background: var(--color-accent);
  opacity: 0.5;
}

/* ── Slide transition ── */
.mp-slide-enter-active,
.mp-slide-leave-active {
  transition: transform 200ms ease, opacity 200ms ease;
}
.mp-slide-enter-from,
.mp-slide-leave-to {
  transform: translateX(100%);
  opacity: 0;
}

/* ── Header ── */
.mp-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px var(--space-3);
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
}

.mp-title {
  font-size: var(--font-size-sm);
  font-weight: 600;
  color: var(--text-primary);
}

.mp-close {
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
  transition: background var(--transition), color var(--transition);
}
.mp-close:hover {
  background: rgba(255, 255, 255, 0.12);
  color: var(--text-primary);
}

/* ── Thumbnail ── */
.mp-thumb-wrap {
  position: relative;
  width: 100%;
  background: #111;
  flex-shrink: 0;
  overflow: hidden;
}

.mp-thumb-resize-handle {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  height: 5px;
  cursor: row-resize;
  z-index: 10;
  transition: background var(--transition);
}
.mp-thumb-resize-handle:hover,
.mp-thumb-resize-handle:active {
  background: var(--color-accent);
  opacity: 0.5;
}

.mp-thumb {
  width: 100%;
  height: 100%;
  object-fit: contain;
  display: block;
}

.mp-thumb-placeholder {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #1a1a1a;
}

.mp-thumb-ext {
  font-size: 13px;
  font-weight: 700;
  color: var(--text-muted);
  background: var(--bg-card);
  padding: 4px 10px;
  border-radius: var(--border-radius-sm);
  border: 1px solid var(--border-color);
}

.mp-thumb-spinner {
  width: 28px;
  height: 28px;
  border: 2px solid var(--border-color);
  border-top-color: var(--color-accent);
  border-radius: 50%;
  animation: mp-spin 0.7s linear infinite;
}

@keyframes mp-spin {
  to { transform: rotate(360deg); }
}

.mp-original-badge {
  position: absolute;
  top: 8px;
  left: 8px;
  background: var(--color-success);
  color: #fff;
  font-size: 10px;
  font-weight: 600;
  padding: 2px 8px;
  border-radius: var(--border-radius-pill);
}

/* ── File header ── */
.mp-file-header {
  padding: var(--space-2) var(--space-3);
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
}

.mp-filename {
  font-size: var(--font-size-sm);
  font-weight: 600;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.mp-filepath {
  font-size: 10px;
  color: var(--text-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  margin-top: 2px;
}

/* ── Loading / error ── */
.mp-loading {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-5);
  color: var(--text-muted);
  font-size: var(--font-size-xs);
}

@keyframes spin { to { transform: rotate(360deg); } }
.mp-spinner {
  width: 24px;
  height: 24px;
  border: 2px solid rgba(255,255,255,0.1);
  border-top-color: var(--color-accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

.mp-error {
  padding: var(--space-3);
  color: var(--color-danger);
  font-size: var(--font-size-xs);
}

/* ── Scrollable content ── */
.mp-content {
  flex: 1;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  scrollbar-width: thin;
  scrollbar-color: rgba(255,255,255,0.1) transparent;
}

/* ── Sections ── */
.mp-section {
  padding: 0 var(--space-3);
  border-bottom: 1px solid var(--border-color);
}


.mp-section--edit {
  border-bottom: none;
}


/* .mp-section-title is defined globally in base.css */


/* Content padding inside each section */
.mp-section > *:not(.mp-section-title) {
  padding-bottom: var(--space-2);
}

/* ── Read-only rows ── */
.mp-rows {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.mp-row {
  display: flex;
  align-items: baseline;
  gap: var(--space-2);
  font-size: var(--font-size-xs);
}

.mp-label {
  color: var(--text-muted);
  flex-shrink: 0;
  width: 90px;
}

.mp-value {
  color: var(--text-secondary);
  word-break: break-word;
}
.mp-value--muted {
  color: var(--text-muted);
  font-style: italic;
}

/* ── Editable rows ── */
.mp-edit-rows {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.mp-edit-row {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.mp-edit-row .mp-label {
  width: auto;
  font-size: 10px;
}

.mp-input {
  width: 100%;
  padding: 5px 8px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-primary);
  font-size: var(--font-size-xs);
  outline: none;
  transition: border-color var(--transition);
  box-sizing: border-box;
}
.mp-input:focus { border-color: var(--color-accent); }
.mp-input::placeholder { color: var(--text-muted); }
.mp-input--error { border-color: var(--color-danger) !important; }

.mp-gps-row {
  display: flex;
  gap: var(--space-2);
}
.mp-gps-row .mp-edit-row {
  flex: 1;
  min-width: 0;
}
.mp-gps-label {
  font-size: var(--font-size-xs) !important;
  width: auto !important;
}

.mp-gps-error {
  font-size: 10px;
  color: var(--color-danger);
  margin-top: 2px;
  width: 100%;
}

/* datetime-local color fix */
.mp-input[type="datetime-local"]::-webkit-calendar-picker-indicator {
  filter: invert(0.6);
  cursor: pointer;
}

/* ── Save actions ── */
/* ── Floating action bar ── */
.mp-float-bar {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  z-index: 20;
  padding: 10px var(--space-3);
  background: var(--bg-secondary);
  border-top: 1px solid var(--border-color);
  box-shadow: 0 -4px 16px rgba(0, 0, 0, 0.25);
}

.mp-bar-enter-active,
.mp-bar-leave-active {
  transition: transform 180ms ease, opacity 180ms ease;
}
.mp-bar-enter-from,
.mp-bar-leave-to {
  transform: translateY(100%);
  opacity: 0;
}

.mp-actions {
  display: flex;
  gap: var(--space-2);
}

.mp-btn {
  flex: 1;
  padding: 5px var(--space-2);
  border-radius: var(--border-radius-sm);
  font-size: var(--font-size-xs);
  font-weight: 500;
  cursor: pointer;
  transition: background var(--transition), color var(--transition), opacity var(--transition);
}
.mp-btn:disabled { opacity: 0.5; cursor: not-allowed; }

.mp-btn-ghost {
  background: transparent;
  color: var(--text-secondary);
  border: 1px solid var(--border-color);
}
.mp-btn-ghost:hover:not(:disabled) { background: var(--bg-card); }

.mp-btn-primary {
  background: var(--color-accent);
  color: #fff;
  border: none;
}
.mp-btn-primary:hover:not(:disabled) { opacity: 0.85; }

/* ── Save notification modal ── */
.mp-notify-overlay {
  position: fixed;
  inset: 0;
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
  pointer-events: none;
}
.mp-notify-card {
  pointer-events: auto;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 14px 22px;
  border-radius: var(--border-radius-lg);
  background: var(--bg-secondary);
  box-shadow: 0 8px 32px rgba(0,0,0,0.5);
  border: 1px solid var(--border-color);
  font-size: var(--font-size-sm);
  font-weight: 500;
  cursor: pointer;
}
.mp-notify-icon { font-size: 16px; line-height: 1; }
.mp-notify--success { border-color: var(--color-success); color: var(--color-success); }
.mp-notify--error   { border-color: var(--color-danger);  color: var(--color-danger);  }
.mp-notify-enter-active,
.mp-notify-leave-active { transition: opacity 0.2s ease, transform 0.2s ease; }
.mp-notify-enter-from,
.mp-notify-leave-to { opacity: 0; transform: scale(0.92); }

.mp-save-notice {
  margin-top: var(--space-2);
  font-size: 10px;
  color: var(--text-muted);
  opacity: 0.7;
}
</style>
