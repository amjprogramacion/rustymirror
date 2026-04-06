<template>
  <Transition name="mp-slide">
    <div v-if="store.metadataPanel" class="mp-panel" :style="{ width: panelWidth + 'px' }" @keydown.escape="store.closeMetadataPanel()">
      <!-- Resize handle -->
      <div class="mp-resize-handle" @mousedown.prevent="startResize" />

      <!-- Header -->
      <div class="mp-header">
        <span class="mp-title">Image Info</span>
        <button class="mp-close" @click="store.closeMetadataPanel()" title="Close">✕</button>
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
          <span class="mp-thumb-ext">{{ fileExt(entry.path).toUpperCase() }}</span>
        </div>
        <span v-if="entry.isOriginal" class="mp-original-badge">Original</span>
      </div>

      <!-- Filename + path -->
      <div class="mp-file-header">
        <p class="mp-filename" :title="entry.path">{{ fileName(entry.path) }}</p>
        <p class="mp-filepath" :title="entry.path">{{ folderPath(entry.path) }}</p>
      </div>

      <!-- Loading state -->
      <div v-if="panel.loading" class="mp-loading">
        <span class="mp-spinner" />
        <span>Reading metadata…</span>
      </div>

      <!-- Error state -->
      <div v-else-if="panel.error" class="mp-error">
        {{ panel.error }}
      </div>

      <!-- Metadata content -->
      <div v-else-if="meta" class="mp-content" :style="panel.dirty || panel.saving ? 'padding-bottom: 64px' : ''">

        <!-- File + Camera side by side -->
        <div class="mp-pair">
          <div class="mp-section mp-section--half">
            <button class="mp-section-title" @click="toggle('file')">
              File <span class="mp-chevron" :class="{ 'mp-chevron--open': !collapsed.file }">›</span>
            </button>
            <div class="mp-rows" v-show="!collapsed.file">
              <div class="mp-row"><span class="mp-label">Size</span><span class="mp-value">{{ formatSize(meta.fileSize) }}</span></div>
              <div class="mp-row" v-if="meta.width > 0"><span class="mp-label">Dims</span><span class="mp-value">{{ meta.width }}×{{ meta.height }}</span></div>
            </div>
          </div>
          <div class="mp-section mp-section--half" v-if="hasCameraInfo">
            <button class="mp-section-title" @click="toggle('camera')">
              Camera <span class="mp-chevron" :class="{ 'mp-chevron--open': !collapsed.camera }">›</span>
            </button>
            <div class="mp-rows" v-show="!collapsed.camera">
              <div class="mp-row" v-if="meta.make || meta.model">
                <span class="mp-label">Device</span>
                <span class="mp-value">{{ [meta.make, meta.model].filter(Boolean).join(' ') }}</span>
              </div>
              <div class="mp-row" v-if="meta.lensModel">
                <span class="mp-label">Lens</span>
                <span class="mp-value">{{ meta.lensModel }}</span>
              </div>
              <div class="mp-row" v-if="meta.software">
                <span class="mp-label">Software</span>
                <span class="mp-value">{{ meta.software }}</span>
              </div>
            </div>
          </div>
        </div>

        <!-- Date -->
        <div class="mp-section">
          <button class="mp-section-title" @click="toggle('date')">
            Date taken <span class="mp-chevron" :class="{ 'mp-chevron--open': !collapsed.date }">›</span>
          </button>
          <div class="mp-edit-rows" v-show="!collapsed.date">
            <label class="mp-edit-row">
              <input
                class="mp-input"
                type="datetime-local"
                step="1"
                :value="isoToDatetimeLocal(edit.dateTimeOriginal)"
                @change="e => { edit.dateTimeOriginal = datetimeLocalToIso(e.target.value); panel.dirty = true }"
              />
            </label>
          </div>
        </div>

        <!-- Location -->
        <div class="mp-section">
          <button class="mp-section-title" @click="toggle('location')">
            Location <span class="mp-chevron" :class="{ 'mp-chevron--open': !collapsed.location }">›</span>
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
                  v-model="edit.gpsLatitudeRaw"
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
                  v-model="edit.gpsLongitudeRaw"
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
            <MapPreview v-if="hasGpsPreview" :lat="previewLat" :lon="previewLon" />
          </div>
        </div>

        <!-- Exposure -->
        <div class="mp-section" v-if="hasExposureInfo">
          <button class="mp-section-title" @click="toggle('exposure')">
            Exposure <span class="mp-chevron" :class="{ 'mp-chevron--open': !collapsed.exposure }">›</span>
          </button>
          <div class="mp-rows" v-show="!collapsed.exposure">
            <div class="mp-row" v-if="meta.exposureTime">
              <span class="mp-label">Shutter</span>
              <span class="mp-value">{{ meta.exposureTime }}</span>
            </div>
            <div class="mp-row" v-if="meta.fNumber">
              <span class="mp-label">Aperture</span>
              <span class="mp-value">{{ meta.fNumber }}</span>
            </div>
            <div class="mp-row" v-if="meta.isoSpeed">
              <span class="mp-label">ISO</span>
              <span class="mp-value">{{ meta.isoSpeed }}</span>
            </div>
            <div class="mp-row" v-if="meta.focalLength">
              <span class="mp-label">Focal length</span>
              <span class="mp-value">{{ meta.focalLength }}</span>
            </div>
            <div class="mp-row" v-if="meta.flash">
              <span class="mp-label">Flash</span>
              <span class="mp-value">{{ meta.flash }}</span>
            </div>
            <div class="mp-row" v-if="meta.whiteBalance">
              <span class="mp-label">White balance</span>
              <span class="mp-value">{{ meta.whiteBalance }}</span>
            </div>
            <div class="mp-row" v-if="meta.exposureMode">
              <span class="mp-label">Exp. mode</span>
              <span class="mp-value">{{ meta.exposureMode }}</span>
            </div>
            <div class="mp-row" v-if="meta.meteringMode">
              <span class="mp-label">Metering</span>
              <span class="mp-value">{{ meta.meteringMode }}</span>
            </div>
          </div>
        </div>

        <!-- Editable fields -->
        <div class="mp-section mp-section--edit">
          <button class="mp-section-title" @click="toggle('details')">
            Details <span class="mp-chevron" :class="{ 'mp-chevron--open': !collapsed.details }">›</span>
          </button>
          <div class="mp-edit-rows" v-show="!collapsed.details">

            <label class="mp-edit-row">
              <span class="mp-label">Description</span>
              <input
                class="mp-input"
                type="text"
                v-model="edit.imageDescription"
                @input="panel.dirty = true"
                placeholder="Add a description…"
              />
            </label>

            <label class="mp-edit-row">
              <span class="mp-label">Artist</span>
              <input
                class="mp-input"
                type="text"
                v-model="edit.artist"
                @input="panel.dirty = true"
                placeholder="Photographer name…"
              />
            </label>

            <label class="mp-edit-row">
              <span class="mp-label">Copyright</span>
              <input
                class="mp-input"
                type="text"
                v-model="edit.copyright"
                @input="panel.dirty = true"
                placeholder="© Year Name…"
              />
            </label>
          </div>

          <p class="mp-save-notice" v-if="!panel.dirty && !panel.saving && !saveError">
            Changes are written directly to the file's EXIF data.
          </p>
        </div>

      </div>

      <!-- Floating action bar -->
      <Transition name="mp-bar">
        <div class="mp-float-bar" v-if="panel.dirty || panel.saving">
          <p v-if="saveError" class="mp-save-error">{{ saveError }}</p>
          <div class="mp-actions">
            <button class="mp-btn mp-btn-ghost" @click="resetEdit" :disabled="panel.saving">Reset</button>
            <button class="mp-btn mp-btn-primary" @click="save" :disabled="panel.saving">
              <span v-if="panel.saving">Saving…</span>
              <span v-else>Save changes</span>
            </button>
          </div>
        </div>
      </Transition>

    </div>
  </Transition>
</template>

<script setup>
import { ref, computed, watch, onBeforeUnmount } from 'vue'
import { convertFileSrc } from '@tauri-apps/api/core'
import { useScanStore } from '../store/scan'
import MapPreview from './MapPreview.vue'
import { fileExt, fileName, folderPath, formatSize, formatGps, isoToDatetimeLocal, datetimeLocalToIso } from '../utils/formatters'

const store = useScanStore()
const HEIC  = new Set(['heic', 'heif'])

const MIN_WIDTH = 450
const panelWidth = ref(MIN_WIDTH)

const MIN_THUMB_HEIGHT = 200
const thumbHeight = ref(MIN_THUMB_HEIGHT)

function startThumbResize(e) {
  const startY = e.clientY
  const startH = thumbHeight.value

  function onMove(e) {
    thumbHeight.value = Math.max(MIN_THUMB_HEIGHT, startH + (e.clientY - startY))
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
    panelWidth.value = Math.max(MIN_WIDTH, startW + delta)
  }
  function onUp() {
    window.removeEventListener('mousemove', onMove)
    window.removeEventListener('mouseup', onUp)
  }
  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
}

onBeforeUnmount(() => {
  panelWidth.value = MIN_WIDTH
  thumbHeight.value = MIN_THUMB_HEIGHT
})

const panel  = computed(() => store.metadataPanel)
const entry  = computed(() => panel.value?.entry ?? {})
const meta   = computed(() => panel.value?.metadata ?? null)

// ── Section collapse state ────────────────────────────────────────────────────
const collapsed = ref({ file: false, camera: false, date: false, location: false, exposure: true, details: false })
function toggle(key) { collapsed.value[key] = !collapsed.value[key] }
// Reset collapse state when a new image is opened
watch(meta, (m) => { if (m) collapsed.value = { file: false, camera: false, date: false, location: false, exposure: true, details: false } })

const saveError = ref(null)

// Reverse geocoding via Nominatim (OpenStreetMap)
const locationName    = ref(null)
const locationLoading = ref(false)

// ── GPS parsing ───────────────────────────────────────────────────────────────
// Accepts decimal ("40.71600", "-74.006") or DMS ("39°48'43.1"N", "0°25'29.1"W")
// Returns decimal degrees, or null if unparseable.
function parseGpsInput(raw) {
  if (!raw || !raw.trim()) return null
  const s = raw.trim()

  // Plain decimal: optional sign, digits, optional decimal part
  if (/^-?\d+(\.\d+)?$/.test(s)) return parseFloat(s)

  // DMS pattern: degrees° [minutes' [seconds"]] [NSEW]
  const dms = s.match(
    /^(\d+(?:\.\d+)?)\s*[°d]\s*(?:(\d+(?:\.\d+)?)\s*['′]\s*(?:(\d+(?:\.\d+)?)\s*["″]\s*)?)?([NSEWnsew])?$/
  )
  if (dms) {
    const deg = parseFloat(dms[1])
    const min = dms[2] ? parseFloat(dms[2]) : 0
    const sec = dms[3] ? parseFloat(dms[3]) : 0
    const dir = (dms[4] || '').toUpperCase()
    let decimal = deg + min / 60 + sec / 3600
    if (dir === 'S' || dir === 'W') decimal = -decimal
    return decimal
  }

  return null
}

// Parse a combined string like "39°48'43.1"N 0°25'29.1"W" → { lat, lon } or null.
function parseCombinedGps(raw) {
  if (!raw || !raw.trim()) return null
  const pattern = /(\d+(?:\.\d+)?)\s*[°d]\s*(?:(\d+(?:\.\d+)?)\s*['′]\s*(?:(\d+(?:\.\d+)?)\s*["″]\s*)?)?([NSEWnsew])/g
  const matches = [...raw.matchAll(pattern)]
  if (matches.length < 2) return null

  let lat = null, lon = null
  for (const m of matches) {
    const deg = parseFloat(m[1])
    const min = m[2] ? parseFloat(m[2]) : 0
    const sec = m[3] ? parseFloat(m[3]) : 0
    const dir = m[4].toUpperCase()
    let dec = deg + min / 60 + sec / 3600
    if (dir === 'S' || dir === 'W') dec = -dec
    if ('NS'.includes(dir)) lat = dec
    else lon = dec
  }

  return (lat !== null && lon !== null) ? { lat, lon } : null
}

// Editable fields — synced from meta when panel opens
const edit = ref({
  dateTimeOriginal: null,
  imageDescription: '',
  artist: '',
  copyright: '',
  gpsLatitudeRaw: '',
  gpsLongitudeRaw: '',
})

const gpsLatError     = ref(null)
const gpsLonError     = ref(null)
const gpsCombinedRaw  = ref('')
const gpsCombinedError = ref(null)

// Show combined single input only when the image has no GPS and the user hasn't
// filled either individual field yet.
const showCombinedInput = computed(() =>
  meta.value?.gpsLatitude == null &&
  !edit.value.gpsLatitudeRaw &&
  !edit.value.gpsLongitudeRaw
)

function onCombinedInput() {
  gpsCombinedError.value = null
  const result = parseCombinedGps(gpsCombinedRaw.value)
  if (result) {
    edit.value.gpsLatitudeRaw  = result.lat.toFixed(6)
    edit.value.gpsLongitudeRaw = result.lon.toFixed(6)
    gpsCombinedRaw.value = ''
    panel.value.dirty = true
  }
}

function resetEdit() {
  if (!meta.value) return
  edit.value = {
    dateTimeOriginal: meta.value.dateTimeOriginal ?? null,
    imageDescription: meta.value.imageDescription ?? '',
    artist:           meta.value.artist ?? '',
    copyright:        meta.value.copyright ?? '',
    gpsLatitudeRaw:   meta.value.gpsLatitude != null ? meta.value.gpsLatitude.toFixed(6) : '',
    gpsLongitudeRaw:  meta.value.gpsLongitude != null ? meta.value.gpsLongitude.toFixed(6) : '',
  }
  gpsLatError.value     = null
  gpsLonError.value     = null
  gpsCombinedRaw.value  = ''
  gpsCombinedError.value = null
  if (panel.value) panel.value.dirty = false
  saveError.value = null
}

function onGpsInput(field) {
  if (field === 'lat') gpsLatError.value = null
  else gpsLonError.value = null
  panel.value.dirty = true
}

// On blur: parse & normalize the raw input to decimal if it's DMS
function normalizeGpsInput(field) {
  const key = field === 'lat' ? 'gpsLatitudeRaw' : 'gpsLongitudeRaw'
  const errKey = field === 'lat' ? gpsLatError : gpsLonError
  const raw = edit.value[key]
  if (!raw || !raw.trim()) return
  const val = parseGpsInput(raw)
  if (val === null) {
    errKey.value = `Invalid ${field === 'lat' ? 'latitude' : 'longitude'}`
    return
  }
  errKey.value = null
  // Normalize to decimal string (round to 6 decimal places)
  edit.value[key] = val.toFixed(6)
}

// Reset editable fields whenever a new panel opens or metadata loads
watch(meta, (m) => { if (m) resetEdit() }, { immediate: true })

// Parsed numeric GPS from raw inputs
const parsedLat = computed(() => parseGpsInput(edit.value.gpsLatitudeRaw))
const parsedLon = computed(() => parseGpsInput(edit.value.gpsLongitudeRaw))

// Preview coordinates: parsed edit values, or fall back to meta
const previewLat = computed(() => parsedLat.value ?? meta.value?.gpsLatitude ?? null)
const previewLon = computed(() => parsedLon.value ?? meta.value?.gpsLongitude ?? null)
const hasGpsPreview = computed(() => previewLat.value != null && previewLon.value != null)

// Reverse geocoding — debounced so Nominatim isn't called on every keystroke
let geocodeTimer = null
watch(
  () => [previewLat.value, previewLon.value],
  ([lat, lon]) => {
    clearTimeout(geocodeTimer)
    if (lat == null || lon == null) { locationName.value = null; return }
    locationLoading.value = true
    geocodeTimer = setTimeout(async () => {
      try {
        const res = await fetch(
          `https://nominatim.openstreetmap.org/reverse?lat=${lat}&lon=${lon}&format=json`,
          { headers: { 'User-Agent': 'RustyMirror/1.0 (desktop app)' } }
        )
        const data = await res.json()
        const addr = data.address ?? {}
        const city = addr.city ?? addr.town ?? addr.village ?? addr.municipality ?? addr.county ?? null
        locationName.value = [city, addr.country].filter(Boolean).join(', ') || null
      } catch {
        locationName.value = null
      } finally {
        locationLoading.value = false
      }
    }, 600)
  },
  { immediate: true }
)

async function save() {
  saveError.value = null

  // Validate GPS before saving
  const rawLat = edit.value.gpsLatitudeRaw.trim()
  const rawLon = edit.value.gpsLongitudeRaw.trim()
  const hasRawGps = rawLat !== '' || rawLon !== ''

  if (hasRawGps) {
    const lat = parseGpsInput(rawLat)
    const lon = parseGpsInput(rawLon)
    if (rawLat && lat === null) { gpsLatError.value = 'Invalid latitude'; return }
    if (rawLon && lon === null) { gpsLonError.value = 'Invalid longitude'; return }
    if (rawLat && (lat < -90 || lat > 90))   { gpsLatError.value = 'Must be between -90 and 90'; return }
    if (rawLon && (lon < -180 || lon > 180)) { gpsLonError.value = 'Must be between -180 and 180'; return }
  }

  const lat = hasRawGps ? parsedLat.value : null
  const lon = hasRawGps ? parsedLon.value : null
  const hasValidGps = lat !== null && lon !== null

  await store.saveMetadata({
    dateTimeOriginal: edit.value.dateTimeOriginal || null,
    imageDescription: edit.value.imageDescription || null,
    artist:           edit.value.artist || null,
    copyright:        edit.value.copyright || null,
    gpsLatitude:      hasValidGps ? lat : null,
    gpsLongitude:     hasValidGps ? lon : null,
  })
  if (panel.value?.error) saveError.value = panel.value.error
}

// Thumbnail source
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

const hasCameraInfo  = computed(() => meta.value && (meta.value.make || meta.value.model || meta.value.lensModel || meta.value.software))
const hasExposureInfo = computed(() => meta.value && (meta.value.exposureTime || meta.value.fNumber || meta.value.isoSpeed || meta.value.focalLength))

// ── Formatters ────────────────────────────────────────────────────────────────
</script>

<style scoped>
.mp-panel {
  position: fixed;
  top: 0;
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

/* ── Side-by-side pair (File + Camera) ── */
.mp-pair {
  display: flex;
  align-items: stretch;
  border-bottom: 1px solid var(--border-color);
}

.mp-section--half {
  flex: 1;
  min-width: 0;
  border-bottom: none;
  padding: 0 var(--space-3);
}

.mp-section--half:first-child {
  border-right: 1px solid var(--border-color);
}

/* Tighter label width inside half columns */
.mp-section--half .mp-label {
  width: 52px;
}

.mp-section-title {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  height: 36px;
  font-size: 10px;
  font-weight: 700;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.6px;
  margin: 0;
  background: none;
  border: none;
  padding: 0;
  cursor: pointer;
  text-align: left;
  line-height: 1;
}
.mp-section-title:hover { color: var(--text-secondary); }

.mp-chevron {
  font-size: 18px;
  font-weight: 400;
  line-height: 1;
  color: var(--text-muted);
  transform: rotate(90deg);
  transition: transform 180ms ease;
  display: flex;
  align-items: center;
  flex-shrink: 0;
}
.mp-chevron--open {
  transform: rotate(270deg);
}

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

.mp-save-error {
  margin-top: var(--space-2);
  font-size: 10px;
  color: var(--color-danger);
}

.mp-save-notice {
  margin-top: var(--space-2);
  font-size: 10px;
  color: var(--text-muted);
  opacity: 0.7;
}
</style>
