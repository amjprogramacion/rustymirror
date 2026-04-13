<template>
  <Transition name="mbp-slide">
    <div
      v-if="panel.activePanel"
      class="mbp-panel"
      :style="{ height: panelHeight + 'px' }"
      @keydown.escape="panel.closePanel()"
    >
      <!-- Resize handle (top edge) -->
      <div class="mbp-resize-handle" @mousedown.prevent="startResize" />

      <!-- Header row -->
      <div class="mbp-header">
        <!-- Single mode: thumbnail + file info -->
        <div class="mbp-left" v-if="!isBatch">
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
        <!-- Batch mode: icon + count -->
        <div class="mbp-left" v-else>
          <div class="mbp-batch-icon">✦</div>
          <div class="mbp-file-info">
            <p class="mbp-filename">Editing {{ panel.entries.length }} images</p>
            <p class="mbp-filepath">Batch EXIF edit</p>
          </div>
        </div>

        <button class="mbp-close" @click="panel.closePanel()" title="Close">✕</button>
      </div>

      <!-- Loading state -->
      <div v-show="panel.loading" class="mbp-loading">
        <span class="mbp-spinner" />
        <span>Reading metadata…</span>
      </div>

      <!-- Error state -->
      <div v-show="panel.error && !panel.loading" class="mbp-error">
        {{ panel.error }}
      </div>

      <!-- ── Unified sections (single + batch) ── -->
      <div v-show="!panel.loading && !panel.error && (meta || (isBatch && batchAgg))" class="mbp-sections-wrap">
        <div class="mbp-sections">

          <!-- File & Camera -->
          <div class="mbp-section">
            <div class="mbp-section-title">File &amp; Camera</div>
            <div class="mbp-rows">
              <div class="mbp-row">
                <span class="mbp-label">Size</span>
                <span class="mbp-value" v-if="!isBatch">{{ formatSize(meta.fileSize) }}</span>
                <span class="mbp-value" v-else-if="batchAgg.fileSize !== MIXED">{{ formatSize(batchAgg.fileSize) }}</span>
                <span class="mbp-value mbp-value--muted" v-else>Various values</span>
              </div>
              <div class="mbp-row" v-if="!isBatch ? meta.width > 0 : batchAgg.width != null">
                <span class="mbp-label">Dims</span>
                <span class="mbp-value" v-if="!isBatch">{{ meta.width }}×{{ meta.height }}</span>
                <span class="mbp-value" v-else-if="batchAgg.width !== MIXED">{{ batchAgg.width }}×{{ batchAgg.height }}</span>
                <span class="mbp-value mbp-value--muted" v-else>Various values</span>
              </div>
              <div class="mbp-row" v-if="!isBatch ? (meta.make || meta.model) : (batchAgg.make != null || batchAgg.model != null)">
                <span class="mbp-label">Device</span>
                <span class="mbp-value" v-if="!isBatch">{{ [meta.make, meta.model].filter(Boolean).join(' ') }}</span>
                <span class="mbp-value" v-else-if="batchAgg.make !== MIXED && batchAgg.model !== MIXED">{{ [batchAgg.make, batchAgg.model].filter(Boolean).join(' ') }}</span>
                <span class="mbp-value mbp-value--muted" v-else>Various values</span>
              </div>
              <div class="mbp-row" v-if="!isBatch ? meta.lensModel : batchAgg.lensModel != null">
                <span class="mbp-label">Lens</span>
                <span class="mbp-value" v-if="!isBatch">{{ meta.lensModel }}</span>
                <span class="mbp-value" v-else-if="batchAgg.lensModel !== MIXED">{{ batchAgg.lensModel }}</span>
                <span class="mbp-value mbp-value--muted" v-else>Various values</span>
              </div>
              <div class="mbp-row" v-if="!isBatch ? meta.software : batchAgg.software != null">
                <span class="mbp-label">Software</span>
                <span class="mbp-value" v-if="!isBatch">{{ meta.software }}</span>
                <span class="mbp-value" v-else-if="batchAgg.software !== MIXED">{{ batchAgg.software }}</span>
                <span class="mbp-value mbp-value--muted" v-else>Various values</span>
              </div>
            </div>
          </div>

          <!-- Date taken -->
          <div class="mbp-section">
            <div class="mbp-section-title">Date taken</div>
            <div class="mbp-edit-rows">
              <label class="mbp-edit-row">
                <input
                  class="mbp-input"
                  type="datetime-local"
                  step="1"
                  :value="isoToDatetimeLocal(isBatch ? batchEdit.dateTimeOriginal : edit.dateTimeOriginal)"
                  :placeholder="isBatch && batchAgg.dateTimeOriginal === MIXED ? 'Various values' : ''"
                  @change="e => { const v = datetimeLocalToIso(e.target.value); isBatch ? (batchEdit.dateTimeOriginal = v) : (edit.dateTimeOriginal = v); panel.dirty = true }"
                />
              </label>
              <p v-if="isBatch && batchAgg.dateTimeOriginal === MIXED && !batchEdit.dateTimeOriginal" class="mbp-various-hint">Various values — leave empty to keep each file's date</p>
            </div>
          </div>

          <!-- Location -->
          <div class="mbp-section mbp-section--location">
            <div class="mbp-location-left">
              <div class="mbp-section-title">Location</div>

              <!-- Single mode: geocoded name + combined/split inputs -->
              <template v-if="!isBatch">
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
                <div v-if="showCombinedInput" class="mbp-edit-rows">
                  <label class="mbp-edit-row">
                    <input class="mbp-input" type="text" v-model="gpsCombinedRaw" @input="onCombinedInput" :class="{ 'mbp-input--error': gpsCombinedError }" placeholder="39°48'43.1&quot;N 0°25'29.1&quot;W" />
                  </label>
                  <p v-if="gpsCombinedError" class="mbp-gps-error">{{ gpsCombinedError }}</p>
                </div>
                <div v-else class="mbp-gps-col">
                  <label class="mbp-edit-row">
                    <span class="mbp-label">Latitude</span>
                    <input class="mbp-input" type="text" v-model="gpsLatitudeRaw" @input="onGpsInput('lat')" @blur="normalizeGpsInput('lat')" :class="{ 'mbp-input--error': gpsLatError }" placeholder="40.71600" />
                  </label>
                  <label class="mbp-edit-row">
                    <span class="mbp-label">Longitude</span>
                    <input class="mbp-input" type="text" v-model="gpsLongitudeRaw" @input="onGpsInput('lon')" @blur="normalizeGpsInput('lon')" :class="{ 'mbp-input--error': gpsLonError }" placeholder="-74.00600" />
                  </label>
                  <p v-if="gpsLatError || gpsLonError" class="mbp-gps-error">{{ gpsLatError || gpsLonError }}</p>
                </div>
              </template>

              <!-- Batch mode: always combined input -->
              <template v-else>
                <div class="mbp-edit-rows">
                  <label class="mbp-edit-row">
                    <input class="mbp-input" type="text" v-model="batchGpsCombinedRaw" @input="onBatchGpsInput" :class="{ 'mbp-input--error': batchGpsCombinedError }" :placeholder="batchAgg.gps.mixed ? 'Various values' : '39°48\'43.1&quot;N 0°25\'29.1&quot;W'" />
                  </label>
                  <p v-if="batchGpsCombinedError" class="mbp-gps-error">{{ batchGpsCombinedError }}</p>
                  <p v-if="batchAgg.gps.mixed && !batchGpsCombinedRaw" class="mbp-various-hint">Various values — leave empty to keep each file's location</p>
                </div>
              </template>
            </div>

            <!-- Map -->
            <div class="mbp-location-map">
              <MapPreview
                ref="mapPreviewRef"
                :lat="isBatch ? batchPreviewLat : singleMapCenter.lat"
                :lon="isBatch ? batchPreviewLon : singleMapCenter.lon"
                :scroll-wheel-zoom="true"
                :show-marker="isBatch ? ((!batchAgg.gps.mixed && batchAgg.gps.lat != null) || batchGpsParsed != null) : hasGpsPreview"
                :reset-key="mapResetKey"
                :saved-view="savedMapView"
                @set-location="onMapSetLocation"
              />
            </div>
          </div>

          <!-- Exposure -->
          <div class="mbp-section" v-if="!isBatch ? hasExposureInfo : hasExposureInfoBatch">
            <div class="mbp-section-title">Exposure</div>
            <div class="mbp-rows">
              <template v-for="row in exposureRows" :key="row.label">
                <div class="mbp-row" v-if="row.visible">
                  <span class="mbp-label">{{ row.label }}</span>
                  <span class="mbp-value" v-if="row.value !== MIXED">{{ row.value }}</span>
                  <span class="mbp-value mbp-value--muted" v-else>Various values</span>
                </div>
              </template>
            </div>
          </div>

          <!-- Details -->
          <div class="mbp-section mbp-section--details">
            <div class="mbp-section-title">Details</div>
            <div class="mbp-edit-rows">
              <label class="mbp-edit-row">
                <span class="mbp-label">Description</span>
                <input class="mbp-input" type="text"
                  :value="isBatch ? batchEdit.imageDescription : edit.imageDescription"
                  @input="e => { isBatch ? (batchEdit.imageDescription = e.target.value) : (edit.imageDescription = e.target.value); panel.dirty = true }"
                  :placeholder="isBatch && batchAgg.imageDescription === MIXED ? 'Various values' : 'Add a description…'" />
              </label>
              <label class="mbp-edit-row">
                <span class="mbp-label">Artist</span>
                <input class="mbp-input" type="text"
                  :value="isBatch ? batchEdit.artist : edit.artist"
                  @input="e => { isBatch ? (batchEdit.artist = e.target.value) : (edit.artist = e.target.value); panel.dirty = true }"
                  :placeholder="isBatch && batchAgg.artist === MIXED ? 'Various values' : 'Photographer name…'" />
              </label>
              <label class="mbp-edit-row">
                <span class="mbp-label">Copyright</span>
                <input class="mbp-input" type="text"
                  :value="isBatch ? batchEdit.copyright : edit.copyright"
                  @input="e => { isBatch ? (batchEdit.copyright = e.target.value) : (edit.copyright = e.target.value); panel.dirty = true }"
                  :placeholder="isBatch && batchAgg.copyright === MIXED ? 'Various values' : '© Year Name…'" />
              </label>
            </div>
            <p class="mbp-save-notice" v-if="!isBatch && !panel.dirty && !panel.saving && !saveError">
              Changes are written directly to the file's EXIF data.
            </p>
          </div>

        </div>

        <!-- Floating action bar -->
        <Transition name="mbp-bar">
          <div class="mbp-float-bar" v-if="panel.dirty || panel.saving">
            <div class="mbp-actions">
              <button class="mbp-btn mbp-btn-ghost" @click="isBatch ? resetBatch() : resetEdit()" :disabled="panel.saving">Reset</button>
              <button class="mbp-btn mbp-btn-primary" @click="isBatch ? saveBatch() : save()" :disabled="panel.saving">
                <span v-if="panel.saving">Saving…</span>
                <span v-else-if="isBatch">Save to {{ panel.entries.length }} images</span>
                <span v-else>Save changes</span>
              </button>
            </div>
          </div>
        </Transition>

      </div>

      <!-- Save notification (centered) -->
      <Teleport to="body">
        <Transition name="mbp-notify">
          <div v-if="notification" class="mbp-notify-overlay" @click="notification = null">
            <div class="mbp-notify-card" :class="`mbp-notify--${notification.type}`" @click.stop>
              <span class="mbp-notify-icon">{{ notification.type === 'success' ? '✓' : '✕' }}</span>
              <span class="mbp-notify-msg">{{ notification.message }}</span>
            </div>
          </div>
        </Transition>
      </Teleport>

    </div>
  </Transition>
</template>

<script setup>
import { ref, computed, watch, onBeforeUnmount } from 'vue'
import { convertFileSrc } from '@tauri-apps/api/core'
import { useDuplicatesStore } from '../store/duplicates'
import { usePanelStore } from '../store/panel'
import { useThumbnailStore } from '../store/thumbnails'
import { useMapViewStore } from '../store/mapView'
import { useMetadataStore } from '../store/metadata'
import MapPreview from './MapPreview.vue'
import { fileExt, fileName, folderPath, formatSize, isoToDatetimeLocal, datetimeLocalToIso } from '../utils/formatters'
import { useGpsEditor, parseCombinedGps } from '../composables/useGpsEditor'

const store     = useDuplicatesStore()
const panel     = usePanelStore()
const thumbs    = useThumbnailStore()
const metaStore = useMetadataStore()
const HEIC      = new Set(['heic', 'heif'])
const MIXED     = '__mixed__'

// ── Panel height resize ───────────────────────────────────────────────────────
const MIN_HEIGHT = 200
const panelHeight = computed({
  get: () => panel.panelHeight,
  set: v => { panel.panelHeight = v }
})

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

const mapViewStore  = useMapViewStore()
const mapPreviewRef = ref(null)

onBeforeUnmount(() => {
  panelHeight.value = 300
  const state = mapPreviewRef.value?.getMapState()
  if (state) mapViewStore.metadata = state
})

// ── Store accessors ───────────────────────────────────────────────────────────
const activePanel = computed(() => panel.activePanel)
const entry = computed(() => activePanel.value?.entry ?? {})
const meta  = computed(() => activePanel.value?.metadata ?? null)

const notification = ref(null)
let   notifyTimer  = null

function showNotification(type, message, duration = 2500) {
  clearTimeout(notifyTimer)
  notification.value = { type, message }
  notifyTimer = setTimeout(() => { notification.value = null }, duration)
}

// ── Batch / single mode ───────────────────────────────────────────────────────
const isBatch = computed(() => panel.activePanel?.batch === true)

// ── Batch: GPS helpers ────────────────────────────────────────────────────────
function decimalToDms(decimal, type) {
  const sign = decimal < 0
  const abs  = Math.abs(decimal)
  const deg  = Math.floor(abs)
  const mf   = (abs - deg) * 60
  const min  = Math.floor(mf)
  const sec  = ((mf - min) * 60).toFixed(1)
  const dir  = type === 'lat' ? (sign ? 'S' : 'N') : (sign ? 'W' : 'E')
  return `${deg}°${min}'${sec}"${dir}`
}
function formatBatchGps(lat, lon) {
  return `${decimalToDms(lat, 'lat')} ${decimalToDms(lon, 'lon')}`
}

// ── Batch: aggregated values ──────────────────────────────────────────────────
const batchAgg = computed(() => {
  if (!isBatch.value || !panel.activePanel?.allMetadata) return null
  const all     = panel.activePanel.allMetadata
  const entries = panel.activePanel.entries

  function agg(fn) {
    const vals  = all.map(fn)
    const first = vals[0]
    return vals.every(v => v === first) ? (first ?? null) : MIXED
  }

  // GPS: compare geocoded location names from the metadata store
  const names    = entries.map(e => metaStore.locationNames[e.path] ?? null)
  const lats     = all.map(m => m.gpsLatitude)
  const hasAny   = lats.some(l => l != null)
  const allHave  = hasAny && lats.every(l => l != null)
  let gps
  if (!hasAny) {
    gps = { mixed: false, lat: null, lon: null }
  } else if (!allHave) {
    gps = { mixed: true, lat: null, lon: null }
  } else {
    const baseLat = all[0].gpsLatitude
    const baseLon = all[0].gpsLongitude
    const coordsSame = all.every(m => m.gpsLatitude === baseLat && m.gpsLongitude === baseLon)
    gps = coordsSame
      ? { mixed: false, lat: baseLat, lon: baseLon }
      : { mixed: true,  lat: null, lon: null }
  }

  return {
    // File & Camera
    fileSize:     agg(m => m.fileSize  ?? null),
    width:        agg(m => m.width     || null),
    height:       agg(m => m.height    || null),
    make:         agg(m => m.make      ?? null),
    model:        agg(m => m.model     ?? null),
    lensModel:    agg(m => m.lensModel ?? null),
    software:     agg(m => m.software  ?? null),
    // Exposure
    exposureTime: agg(m => m.exposureTime ?? null),
    fNumber:      agg(m => m.fNumber      ?? null),
    isoSpeed:     agg(m => m.isoSpeed     ?? null),
    focalLength:  agg(m => m.focalLength  ?? null),
    flash:        agg(m => m.flash        ?? null),
    whiteBalance: agg(m => m.whiteBalance ?? null),
    exposureMode: agg(m => m.exposureMode ?? null),
    meteringMode: agg(m => m.meteringMode ?? null),
    // Editable
    dateTimeOriginal: agg(m => m.dateTimeOriginal ?? null),
    imageDescription: agg(m => m.imageDescription ?? null),
    artist:           agg(m => m.artist           ?? null),
    copyright:        agg(m => m.copyright        ?? null),
    gps,
  }
})

// ── Batch: edit state ─────────────────────────────────────────────────────────
const batchEdit = ref({ dateTimeOriginal: null, imageDescription: null, artist: null, copyright: null })
const batchGpsCombinedRaw   = ref('')
const batchGpsCombinedError = ref(null)

function resetBatch() {
  const agg = batchAgg.value
  if (!agg || !panel.activePanel) return
  batchEdit.value = {
    dateTimeOriginal: agg.dateTimeOriginal === MIXED ? null : agg.dateTimeOriginal,
    imageDescription: agg.imageDescription === MIXED ? null : agg.imageDescription,
    artist:           agg.artist           === MIXED ? null : agg.artist,
    copyright:        agg.copyright        === MIXED ? null : agg.copyright,
  }
  batchGpsCombinedRaw.value   = (!agg.gps.mixed && agg.gps.lat != null) ? formatBatchGps(agg.gps.lat, agg.gps.lon) : ''
  batchGpsCombinedError.value = null
  panel.activePanel.dirty     = false
}

watch(batchAgg, (agg) => { if (agg) resetBatch() }, { immediate: true })

function onBatchGpsInput() {
  batchGpsCombinedError.value = null
  if (panel.activePanel) panel.activePanel.dirty = true
}

const batchGpsParsed    = computed(() => parseCombinedGps(batchGpsCombinedRaw.value))

const SPAIN_CENTER = { lat: 40.416775, lon: -3.703790 }

// Single mode: use GPS coords if available, Spain otherwise
const singleMapCenter = computed(() => ({
  lat: previewLat.value ?? SPAIN_CENTER.lat,
  lon: previewLon.value ?? SPAIN_CENTER.lon,
}))

// Always show the map in batch mode: use typed coords → first image with GPS → Spain as fallback
const batchMapCenter = computed(() => {
  if (batchGpsParsed.value) return batchGpsParsed.value
  const first = panel.activePanel?.allMetadata?.find(m => m.gpsLatitude != null)
  return first ? { lat: first.gpsLatitude, lon: first.gpsLongitude } : SPAIN_CENTER
})

const batchPreviewLat    = computed(() => batchMapCenter.value.lat)
const batchPreviewLon    = computed(() => batchMapCenter.value.lon)
const batchHasGpsPreview = computed(() => isBatch.value)

// resetKey: increments when the map should fully reset (new image in single, panel open/close)
const mapResetKey  = ref(0)
const savedMapView = ref(mapViewStore.metadata)

// Save map state the moment the panel closes (before v-if removes MapPreview from DOM)
watch(() => panel.activePanel, (p, prev) => {
  if (prev && !p) {
    const state = mapPreviewRef.value?.getMapState()
    if (state) mapViewStore.metadata = state // keep for tool-switch restore
    savedMapView.value = null                        // reset for same-tool reopen
  }
}, { flush: 'sync' })

watch(meta, (m, prev) => { if (!isBatch.value && m !== prev) mapResetKey.value++ })

async function saveBatch() {
  batchGpsCombinedError.value = null

  let lat = null, lon = null
  if (batchGpsCombinedRaw.value.trim()) {
    const parsed = parseCombinedGps(batchGpsCombinedRaw.value)
    if (!parsed) { batchGpsCombinedError.value = 'Invalid coordinates'; return }
    lat = parsed.lat
    lon = parsed.lon
  }

  await store.saveBatchMetadata({
    dateTimeOriginal: batchEdit.value.dateTimeOriginal || null,
    imageDescription: batchEdit.value.imageDescription || null,
    artist:           batchEdit.value.artist           || null,
    copyright:        batchEdit.value.copyright        || null,
    gpsLatitude:      lat,
    gpsLongitude:     lon,
  })
  if (panel.activePanel?.error) {
    showNotification('error', panel.activePanel.error, 4000)
  } else {
    showNotification('success', `Saved to ${panel.activePanel?.entries?.length ?? ''} images`)
  }
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
} = useGpsEditor(meta, () => { panel.value.dirty = true })

function onMapSetLocation({ lat, lon }) {
  if (isBatch.value) {
    batchGpsCombinedRaw.value = `${lat.toFixed(6)} ${lon.toFixed(6)}`
    onBatchGpsInput()
  } else {
    gpsLatitudeRaw.value  = lat.toFixed(6)
    gpsLongitudeRaw.value = lon.toFixed(6)
    onGpsInput('lat')
    onGpsInput('lon')
  }
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
  if (panel.value) panel.value.dirty = false
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
  if (panel.value?.error) {
    showNotification('error', panel.value.error, 4000)
  } else {
    showNotification('success', 'Saved successfully')
  }
}

// ── Thumbnail source ──────────────────────────────────────────────────────────
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

const hasCameraInfo   = computed(() => meta.value && (meta.value.make || meta.value.model || meta.value.lensModel || meta.value.software))
const hasExposureInfo = computed(() => meta.value && (meta.value.exposureTime || meta.value.fNumber || meta.value.isoSpeed || meta.value.focalLength))

const hasExposureInfoBatch = computed(() => {
  const a = batchAgg.value
  return a && [a.exposureTime, a.fNumber, a.isoSpeed, a.focalLength, a.flash, a.whiteBalance, a.exposureMode, a.meteringMode].some(v => v != null)
})

// Unified exposure row list — works for both single and batch
const exposureRows = computed(() => {
  const a = batchAgg.value
  const m = meta.value
  function row(label, singleVal, batchField) {
    const bv = a?.[batchField]
    const visible = isBatch.value ? bv != null : !!singleVal
    const value   = isBatch.value ? bv : singleVal
    return { label, value, visible }
  }
  return [
    row('Shutter',       m?.exposureTime, 'exposureTime'),
    row('Aperture',      m?.fNumber,      'fNumber'),
    row('ISO',           m?.isoSpeed,     'isoSpeed'),
    row('Focal length',  m?.focalLength,  'focalLength'),
    row('Flash',         m?.flash,        'flash'),
    row('White balance', m?.whiteBalance, 'whiteBalance'),
    row('Exp. mode',     m?.exposureMode, 'exposureMode'),
    row('Metering',      m?.meteringMode, 'meteringMode'),
  ]
})
</script>

<style scoped>
/* ── Panel ── */
.mbp-panel {
  position: absolute;
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
  flex: 1 0 300px;
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
  height: 26px;
  padding: 0 7px;
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

/* ── Save notification modal ── */
.mbp-notify-overlay {
  position: fixed;
  inset: 0;
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
  pointer-events: none;
}
.mbp-notify-card {
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
.mbp-notify-icon {
  font-size: 16px;
  line-height: 1;
}
.mbp-notify--success { border-color: var(--color-success); color: var(--color-success); }
.mbp-notify--error   { border-color: var(--color-danger);  color: var(--color-danger);  }

.mbp-notify-enter-active,
.mbp-notify-leave-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
}
.mbp-notify-enter-from,
.mbp-notify-leave-to {
  opacity: 0;
  transform: scale(0.92);
}

.mbp-save-notice {
  font-size: 10px;
  color: var(--text-muted);
  opacity: 0.7;
  margin-top: var(--space-1);
}

/* ── Batch mode ── */
.mbp-batch-icon {
  width: 36px;
  height: 36px;
  flex-shrink: 0;
  border-radius: var(--border-radius-sm);
  background: rgba(var(--color-accent-rgb, 99, 102, 241), 0.15);
  border: 1px solid var(--color-accent);
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--color-accent);
  font-size: 16px;
}

.mbp-various-hint {
  font-size: 10px;
  color: var(--text-muted);
  font-style: italic;
}
</style>
