<template>
  <div class="map-wrap">
    <div ref="mapEl" class="map-container" />

    <!-- Custom zoom buttons -->
    <div class="map-zoom">
      <button class="map-zoom-btn" @click="zoomIn" title="Zoom in">+</button>
      <button class="map-zoom-btn" @click="zoomOut" title="Zoom out">−</button>
    </div>

    <!-- Satellite toggle -->
    <button class="map-sat-btn" @click="toggleSatellite" :title="isSatellite ? 'Map view' : 'Satellite view'">
      <span v-if="isSatellite">🗺</span>
      <span v-else>🛰</span>
    </button>

  </div>
</template>

<script setup>
import { ref, onMounted, onBeforeUnmount, watch } from 'vue'
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

const props = defineProps({
  lat:             { type: Number,  required: true },
  lon:             { type: Number,  required: true },
  scrollWheelZoom: { type: Boolean, default: false },
})

const mapEl      = ref(null)
const isSatellite = ref(false)
let map    = null
let marker = null
let tileLayer = null

function initMap() {
  if (!mapEl.value || map) return
  map = L.map(mapEl.value, {
    zoomControl: false,
    scrollWheelZoom: props.scrollWheelZoom,
    attributionControl: false,
  }).setView([props.lat, props.lon], 14)

  tileLayer = L.tileLayer(TILES.map, { maxZoom: 19 }).addTo(map)
  marker = L.marker([props.lat, props.lon]).addTo(map)
}

function toggleSatellite() {
  if (!map) return
  isSatellite.value = !isSatellite.value
  tileLayer.setUrl(isSatellite.value ? TILES.sat : TILES.map)
}

function zoomIn()  { map?.zoomIn() }
function zoomOut() { map?.zoomOut() }

function updateView() {
  if (!map) return
  map.setView([props.lat, props.lon], 14)
  marker?.setLatLng([props.lat, props.lon])
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

watch(() => [props.lat, props.lon], updateView)
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
</style>
