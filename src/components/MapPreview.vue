<template>
  <div class="map-wrap">
    <div ref="mapEl" class="map-container" />

    <!-- Custom zoom buttons — avoids conflict with app's global button/anchor reset -->
    <div class="map-zoom">
      <button class="map-zoom-btn" @click="zoomIn" title="Zoom in">+</button>
      <button class="map-zoom-btn" @click="zoomOut" title="Zoom out">−</button>
    </div>

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

const props = defineProps({
  lat: { type: Number, required: true },
  lon: { type: Number, required: true },
})

const mapEl = ref(null)
let map = null
let marker = null

function initMap() {
  if (!mapEl.value || map) return
  map = L.map(mapEl.value, {
    zoomControl: false,       // disabled — using custom Vue buttons instead
    scrollWheelZoom: false,
    attributionControl: false,
  }).setView([props.lat, props.lon], 14)

  L.tileLayer('https://tile.openstreetmap.org/{z}/{x}/{y}.png', {
    maxZoom: 19,
  }).addTo(map)

  marker = L.marker([props.lat, props.lon]).addTo(map)
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

/* Custom zoom controls — z-index must exceed Leaflet's marker pane (600+) */
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

</style>
