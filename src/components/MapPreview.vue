<template>
  <div class="map-wrap">
    <div ref="mapEl" class="map-container" />

    <!-- Custom zoom buttons -->
    <div class="map-zoom">
      <button class="map-zoom-btn" @click="zoomIn" title="Zoom in">+</button>
      <button class="map-zoom-btn" @click="zoomOut" title="Zoom out">−</button>
    </div>

    <!-- Location search -->
    <div class="map-search" :class="{ open: searchOpen }">
      <button
        v-if="!searchOpen"
        class="map-search-btn"
        @click="openSearch"
        title="Search a place"
      >🔍</button>
      <div v-else class="map-search-box">
        <input
          ref="searchInput"
          v-model="searchQuery"
          class="map-search-input"
          type="text"
          placeholder="Search a place…"
          @keydown.enter.prevent="doSearch"
          @keydown.esc.prevent="closeSearch"
        />
        <button class="map-search-go" @click="doSearch" title="Search">🔍</button>
        <button class="map-search-close" @click="closeSearch" title="Close">✕</button>
        <ul v-if="searching || searchError || searchResults.length" class="map-search-results">
          <li v-if="searching" class="map-search-msg">Searching…</li>
          <li v-else-if="searchError" class="map-search-msg">{{ searchError }}</li>
          <li
            v-for="r in searchResults"
            :key="r.id"
            class="map-search-result"
            @click="selectResult(r)"
          >{{ r.label }}</li>
        </ul>
      </div>
    </div>

    <!-- Satellite toggle -->
    <button class="map-sat-btn" @click="toggleSatellite" :title="isSatellite ? 'Map view' : 'Satellite view'">
      <span v-if="isSatellite">🗺</span>
      <span v-else>🛰</span>
    </button>

  </div>

  <!-- Context menu -->
  <Teleport to="body">
    <div
      v-if="ctxMenu.visible"
      class="map-ctx-menu"
      :style="{ left: ctxMenu.x + 'px', top: ctxMenu.y + 'px' }"
      @click.stop
    >
      <button class="map-ctx-item" @click="applyLocation">Set location here</button>
    </div>
  </Teleport>

</template>

<script setup>
import { ref, nextTick, onMounted, onBeforeUnmount, watch } from 'vue'
import L from 'leaflet'
import 'leaflet/dist/leaflet.css'

import markerIcon2x from 'leaflet/dist/images/marker-icon-2x.png'
import markerIcon from 'leaflet/dist/images/marker-icon.png'
import markerShadow from 'leaflet/dist/images/marker-shadow.png'

delete L.Icon.Default.prototype._getIconUrl
L.Icon.Default.mergeOptions({
  iconRetinaUrl: markerIcon2x,
  iconUrl: markerIcon,
  shadowUrl: markerShadow,
})

const TILES = {
  map: 'https://tile.openstreetmap.org/{z}/{x}/{y}.png',
  sat: 'https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}',
}

const ZOOM_GPS   = 16
const ZOOM_WORLD = 5

const props = defineProps({
  lat:             { type: Number,  required: true },
  lon:             { type: Number,  required: true },
  scrollWheelZoom: { type: Boolean, default: false },
  showMarker:      { type: Boolean, default: true },
  resetKey:        { type: [Number, String], default: null },
  savedView:       { type: Object,  default: null }, // { zoom, lat, lon } restored from store
})

const emit = defineEmits(['set-location'])

const mapEl       = ref(null)
const isSatellite = ref(false)
const ctxMenu     = ref({ visible: false, x: 0, y: 0, lat: 0, lon: 0 })

// Location search (Nominatim / OpenStreetMap geocoding)
const searchOpen    = ref(false)
const searchQuery   = ref('')
const searchResults = ref([])
const searching     = ref(false)
const searchError   = ref('')
const searchInput   = ref(null)
const ZOOM_SEARCH   = 14
let map       = null
let marker    = null
let tileLayer = null

function zoomForState() { return props.showMarker ? ZOOM_GPS : ZOOM_WORLD }

function initMap() {
  if (!mapEl.value || map) return
  const initLat  = props.savedView?.lat  ?? props.lat
  const initLon  = props.savedView?.lon  ?? props.lon
  const initZoom = props.savedView?.zoom ?? zoomForState()
  map = L.map(mapEl.value, {
    zoomControl: false,
    scrollWheelZoom: props.scrollWheelZoom,
    attributionControl: false,
  }).setView([initLat, initLon], initZoom)

  tileLayer = L.tileLayer(TILES.map, { maxZoom: 19 }).addTo(map)
  if (props.showMarker) {
    marker = L.marker([props.lat, props.lon]).addTo(map)
  }

  map.on('contextmenu', (e) => {
    e.originalEvent.preventDefault()
    const rect = mapEl.value.getBoundingClientRect()
    ctxMenu.value = {
      visible: true,
      x: rect.left + e.containerPoint.x,
      y: rect.top + e.containerPoint.y,
      lat: e.latlng.lat,
      lon: e.latlng.lng,
    }
  })
  map.on('click', () => { ctxMenu.value.visible = false })
}

function toggleSatellite() {
  if (!map) return
  isSatellite.value = !isSatellite.value
  tileLayer.setUrl(isSatellite.value ? TILES.sat : TILES.map)
}

function zoomIn()  { map?.zoomIn() }
function zoomOut() { map?.zoomOut() }

function applyLocation() {
  emit('set-location', { lat: ctxMenu.value.lat, lon: ctxMenu.value.lon })
  ctxMenu.value.visible = false
}

function openSearch() {
  searchOpen.value = true
  nextTick(() => searchInput.value?.focus())
}

function closeSearch() {
  searchOpen.value = false
  searchQuery.value = ''
  searchResults.value = []
  searchError.value = ''
  searching.value = false
}

async function doSearch() {
  const q = searchQuery.value.trim()
  if (!q || searching.value) return
  searching.value = true
  searchError.value = ''
  searchResults.value = []
  try {
    const url = 'https://nominatim.openstreetmap.org/search'
      + `?format=jsonv2&limit=6&q=${encodeURIComponent(q)}`
    const res = await fetch(url, { headers: { Accept: 'application/json' } })
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    const data = await res.json()
    searchResults.value = data.map((d) => ({
      id:    d.place_id,
      label: d.display_name,
      lat:   parseFloat(d.lat),
      lon:   parseFloat(d.lon),
    }))
    if (!searchResults.value.length) searchError.value = 'No results'
  } catch {
    searchError.value = 'Search failed'
  } finally {
    searching.value = false
  }
}

function selectResult(r) {
  map?.setView([r.lat, r.lon], ZOOM_SEARCH)
  closeSearch()
}

let resizeObserver = null

onMounted(() => {
  initMap()
  resizeObserver = new ResizeObserver(() => { map?.invalidateSize() })
  resizeObserver.observe(mapEl.value)
})

onBeforeUnmount(() => {
  resizeObserver?.disconnect()
  map?.remove()
  map = null
})

// Pan to new coords when the parent changes them (no zoom change)
watch([() => props.lat, () => props.lon], () => {
  if (!map) return
  map.panTo([props.lat, props.lon])
  marker?.setLatLng([props.lat, props.lon])
})

// Add/remove marker and zoom in when GPS status changes
watch(() => props.showMarker, (show) => {
  if (!map) return
  if (show && !marker) {
    marker = L.marker([props.lat, props.lon]).addTo(map)
    map.setView([props.lat, props.lon], ZOOM_GPS)
  } else if (!show && marker) {
    marker.remove()
    marker = null
  }
})

// Full reset when parent signals a new context (new image, panel reopened…)
watch(() => props.resetKey, () => {
  if (!map) return
  map.setView([props.lat, props.lon], zoomForState())
})

defineExpose({
  getMapState: () => map
    ? { zoom: map.getZoom(), lat: map.getCenter().lat, lon: map.getCenter().lng }
    : null,
})
</script>

<style scoped>
.map-wrap {
  position: relative;
  width: 100%;
  height: 180px;
  border-radius: var(--border-radius-sm);
  overflow: hidden;
  border: 1px solid var(--border-color);
  margin-top: var(--space-2);
  isolation: isolate;
}

.map-container {
  width: 100%;
  height: 100%;
}

/* Custom zoom controls */
.map-zoom {
  position: absolute;
  top: 8px;
  left: 8px;
  z-index: 1000;
  display: flex;
  flex-direction: column;
  border-radius: 4px;
  overflow: hidden;
  border: 1px solid rgba(0, 0, 0, 0.35);
  box-shadow: 0 1px 5px rgba(0, 0, 0, 0.4);
  pointer-events: all;
}

.map-zoom-btn {
  width: 26px;
  height: 26px;
  background: #fff !important;
  color: #333 !important;
  font-size: 18px;
  font-weight: 700;
  line-height: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none !important;
  border-radius: 0 !important;
  padding: 0 !important;
  margin: 0 !important;
  cursor: pointer;
  transition: background 0.15s;
  opacity: 1 !important;
}
.map-zoom-btn:first-child {
  border-bottom: 1px solid rgba(0, 0, 0, 0.2) !important;
}
.map-zoom-btn:hover {
  background: #f0f0f0 !important;
}

/* Satellite toggle button */
.map-sat-btn {
  position: absolute;
  top: 8px;
  right: 8px;
  z-index: 1000;
  width: 26px;
  height: 26px;
  background: #fff !important;
  border: 1px solid rgba(0, 0, 0, 0.35) !important;
  border-radius: 4px !important;
  box-shadow: 0 1px 5px rgba(0, 0, 0, 0.4);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 14px;
  padding: 0 !important;
  margin: 0 !important;
  transition: background 0.15s;
  opacity: 1 !important;
}
.map-sat-btn:hover {
  background: #f0f0f0 !important;
}

/* Location search */
.map-search {
  position: absolute;
  top: 8px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 1000;
  pointer-events: all;
}

.map-search-btn {
  width: 26px;
  height: 26px;
  background: #fff !important;
  border: 1px solid rgba(0, 0, 0, 0.35) !important;
  border-radius: 4px !important;
  box-shadow: 0 1px 5px rgba(0, 0, 0, 0.4);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 13px;
  padding: 0 !important;
  margin: 0 !important;
  transition: background 0.15s;
  opacity: 1 !important;
}
.map-search-btn:hover {
  background: #f0f0f0 !important;
}

.map-search-box {
  display: flex;
  align-items: center;
  background: #fff;
  border: 1px solid rgba(0, 0, 0, 0.35);
  border-radius: 4px;
  box-shadow: 0 1px 5px rgba(0, 0, 0, 0.4);
  overflow: visible;
}

.map-search-input {
  width: 180px;
  height: 26px;
  border: none !important;
  outline: none !important;
  padding: 0 8px !important;
  margin: 0 !important;
  font-size: 12px;
  color: #333 !important;
  background: transparent !important;
}

.map-search-go,
.map-search-close {
  width: 26px;
  height: 26px;
  background: #fff !important;
  color: #333 !important;
  border: none !important;
  border-left: 1px solid rgba(0, 0, 0, 0.2) !important;
  border-radius: 0 !important;
  padding: 0 !important;
  margin: 0 !important;
  font-size: 12px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: background 0.15s;
  opacity: 1 !important;
}
.map-search-go:hover,
.map-search-close:hover {
  background: #f0f0f0 !important;
}

.map-search-results {
  position: absolute;
  top: 30px;
  left: 0;
  right: 0;
  list-style: none;
  margin: 0;
  padding: 0;
  background: #fff;
  border: 1px solid rgba(0, 0, 0, 0.2);
  border-radius: 4px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
  max-height: 160px;
  overflow-y: auto;
}

.map-search-result,
.map-search-msg {
  padding: 6px 9px;
  font-size: 11px;
  color: #333;
  line-height: 1.3;
  border-bottom: 1px solid rgba(0, 0, 0, 0.06);
}
.map-search-result {
  cursor: pointer;
}
.map-search-result:hover {
  background: #f0f0f0;
}
.map-search-result:last-child {
  border-bottom: none;
}
.map-search-msg {
  color: #888;
  cursor: default;
}

</style>

<style>
/* Context menu — global because it's Teleported to body */
.map-ctx-menu {
  position: fixed;
  z-index: 1001;
  background: #fff;
  border: 1px solid rgba(0, 0, 0, 0.2);
  border-radius: 4px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
  min-width: 150px;
  overflow: hidden;
  pointer-events: all;
}

.map-ctx-item {
  display: block;
  width: 100%;
  padding: 7px 12px;
  font-size: 12px;
  color: #333;
  background: none;
  border: none;
  text-align: left;
  cursor: pointer;
  white-space: nowrap;
}

.map-ctx-item:hover {
  background: #f0f0f0;
}
</style>
