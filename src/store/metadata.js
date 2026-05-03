import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { load } from '@tauri-apps/plugin-store'
import { useSettings } from '../composables/useSettings'
import { errorMessage } from '../utils/errors'
import { fileName } from '../utils/formatters'
import { useMetadataHistoryStore } from './metadataHistory'

const STORE_FILE = 'rustymirror.json'
const GEO_CACHE_KEY = 'geoCache'
const CUSTOM_LOCATIONS_KEY = 'customLocations'
const DISCOVERED_LOCATIONS_KEY = 'discoveredLocations'
const CUSTOM_DEVICES_KEY = 'customDevices'
const DISCOVERED_DEVICES_KEY = 'discoveredDevices'
const DEVICE_ALIASES_KEY = 'deviceAliases'

let _geocodeAbortController = null
let _store = null

async function getStore() {
  if (!_store) _store = await load(STORE_FILE, { autoSave: true })
  return _store
}

async function loadGeoCache() {
  try {
    const store = await getStore()
    return (await store.get(GEO_CACHE_KEY)) ?? {}
  } catch { return {} }
}

async function saveGeoCache(cache) {
  try {
    const store = await getStore()
    await store.set(GEO_CACHE_KEY, cache)
  } catch { /* ignore */ }
}

function geoCacheStats(cache) {
  const count = Object.keys(cache).length
  const bytes = new TextEncoder().encode(JSON.stringify(cache)).length
  return { count, bytes }
}

export const useMetadataStore = defineStore('metadata', {
  state: () => ({
    folders: [],
    scanning: false,
    geoCacheCount: 0,
    geoCacheBytes: 0,
    customLocations: [],
    discoveredLocations: [],
    customDevices: [],
    discoveredDevices: [],
    deviceAliases: {}, // { "Make Model": "Alias" }
    scanDone: false,
    activeHistoryEntryId: null,
    images: [],
    searchQuery: '',
    sortBy:  'date',  // 'date' | 'location' | 'device'
    sortDir: 'asc',   // 'asc'  | 'desc'
    locationNames: {}, // path → "City, Country" (populated in background after scan)
    geocoding: false,
    geocodingManual: false,
    devicesManual: false,
    filterDateFrom: '',   // 'YYYY-MM-DD' or ''
    filterDateTo:   '',
    filterLocation: '',   // exact location name or '' = all
    filterDevice:   '',   // exact device string  or '' = all
    error: null,
    failedFiles: [],
    multiSelect: false,
    selected: new Set(),
    networkFolders: new Set(),
    scanProgress: { total: 0, processed: 0 },
    heicProgress: { analyzed: 0, total: 0, phase: '' },
  }),

  getters: {
    availableLocations(state) {
      const seen = new Set()
      for (const name of Object.values(state.locationNames)) {
        if (name) seen.add(name)
      }
      return [...seen].sort((a, b) =>
        a.localeCompare(b, undefined, { sensitivity: 'base', numeric: true }))
    },

    availableDevices(state) {
      const seen = new Set()
      for (const img of state.images) {
        if (img.device) seen.add(img.device)
      }
      return [...seen].sort((a, b) =>
        a.localeCompare(b, undefined, { sensitivity: 'base', numeric: true }))
    },

    filteredImages(state) {
      const q     = state.searchQuery.trim().toLowerCase()
      const from  = state.filterDateFrom
      const to    = state.filterDateTo
      const loc   = state.filterLocation
      const dev   = state.filterDevice

      let list = state.images.filter(e => {
        // Search query
        if (q) {
          if (!fileName(e.path).toLowerCase().includes(q)) return false
        }
        // Date range (compare first 10 chars — "YYYY-MM-DD")
        const dateStr = (e.dateTaken ?? e.modified ?? '').slice(0, 10)
        if (from && dateStr < from) return false
        if (to   && dateStr > to)   return false
        // Location
        if (loc === '__no_location__') {
          if (state.locationNames[e.path]) return false
        } else if (loc && (state.locationNames[e.path] ?? '') !== loc) return false
        // Device
        if (dev && (e.device ?? '') !== dev) return false
        return true
      })

      const dir = state.sortDir === 'asc' ? 1 : -1
      const collator = new Intl.Collator(undefined, { sensitivity: 'base', numeric: true })

      list.sort((a, b) => {
        let va, vb
        if (state.sortBy === 'date') {
          // Prefer EXIF date, fall back to file modified
          va = a.dateTaken ?? a.modified ?? ''
          vb = b.dateTaken ?? b.modified ?? ''
          return va < vb ? -dir : va > vb ? dir : 0
        }
        if (state.sortBy === 'location') {
          // Use geocoded name if available, fall back to null for images without GPS
          va = state.locationNames[a.path] ?? (a.gpsLatitude != null ? '\x00' : null)
          vb = state.locationNames[b.path] ?? (b.gpsLatitude != null ? '\x00' : null)
          // No GPS → always last
          if (!va && !vb) return 0
          if (!va) return 1
          if (!vb) return -1
          // '\x00' = GPS present but name not yet fetched → before real names, stable
          if (va === '\x00' && vb === '\x00') return 0
          if (va === '\x00') return -dir
          if (vb === '\x00') return dir
          return collator.compare(va, vb) * dir
        }
        if (state.sortBy === 'device') {
          va = a.device ?? ''
          vb = b.device ?? ''
          // No device info → always last
          if (!va && !vb) return 0
          if (!va) return 1
          if (!vb) return -1
          return collator.compare(va, vb) * dir
        }
        return 0
      })

      return list
    },

    selectedCount(state) {
      return state.selected.size
    },
  },

  actions: {
    addFolder(path) {
      if (!this.folders.includes(path)) this.folders.push(path)
    },
    removeFolder(path) {
      this.folders = this.folders.filter(f => f !== path)
    },

    // Patch an entry in `images` with fresh values from a read_metadata result,
    // so sorting/filtering reflect the saved changes immediately.
    updateEntryFromMetadata(path, metadata) {
      const idx = this.images.findIndex(e => e.path === path)
      if (idx === -1) return
      const entry = this.images[idx]
      const device = [metadata.make, metadata.model].filter(Boolean).join(' ') || undefined
      if (device) this.saveDiscoveredDevice(device)
      this.images[idx] = {
        ...entry,
        dateTaken:    metadata.dateTimeOriginal ?? entry.dateTaken,
        gpsLatitude:  metadata.gpsLatitude  ?? null,
        gpsLongitude: metadata.gpsLongitude ?? null,
        device:       device ?? entry.device,
      }
    },

    toggleSelected(path) {
      const next = new Set(this.selected)
      if (next.has(path)) next.delete(path)
      else next.add(path)
      this.selected = next
    },
    clearSelection() {
      this.selected = new Set()
    },

    async startScan() {
      this.scanning = true
      this.scanDone = false
      this.images = []
      this.searchQuery = ''
      this.selected = new Set()
      this.multiSelect = false
      this.networkFolders = new Set()
      this.locationNames = {}
      this.filterDateFrom = ''
      this.filterDateTo   = ''
      this.filterLocation = ''
      this.filterDevice   = ''
      this.error = null
      this.failedFiles = []
      this.activeHistoryEntryId = null
      this.scanProgress = { total: 0, processed: 0 }
      this.heicProgress = { analyzed: 0, total: 0, phase: '' }
      _geocodeAbortController = null

      const unlistenProgress = await listen('meta_scan_progress', (e) => {
        this.scanProgress = e.payload
      })
      const unlistenHeic = await listen('meta_analyze_progress', (e) => {
        this.heicProgress = e.payload
      })

      try {
        // Check fingerprint to detect cache hits
        let fingerprint = null
        try { fingerprint = await invoke('directory_fingerprint', { paths: [...this.folders] }) } catch { /* ignore */ }

        const metaHistory = useMetadataHistoryStore()
        const cached = fingerprint ? metaHistory.getCached(this.folders, fingerprint) : null

        const scanStart = Date.now()
        let images
        if (cached) {
          images = cached
        } else {
          const result = await invoke('scan_for_metadata', { paths: this.folders })
          images = result.images
          this.failedFiles = result.failedFiles || []
        }
        const durationMs = cached ? null : (Date.now() - scanStart)

        this.images = images
        for (const img of images) {
          if (img.device) this.saveDiscoveredDevice(img.device)
        }

        try {
          const checks = await Promise.all(this.folders.map(f => invoke('is_network_path', { path: f })))
          this.networkFolders = new Set(this.folders.filter((_, i) => checks[i]))
        } catch { /* ignore */ }

        // Save to history
        const entryId = await metaHistory.addEntry(
          this.folders, images.length, images, fingerprint, durationMs
        )
        this.activeHistoryEntryId = entryId

        const { prefetchFilters } = useSettings()
        if (prefetchFilters.value) {
          // Prefetch ON: wait for all geocoding before marking scan as done.
          // scanning stays true → Stop button remains active during geocoding.
          await this.geocodeAll()
          this.scanDone = true
        } else {
          // Prefetch OFF: show results immediately, geocode in background.
          this.scanDone = true
          this.geocodeAll()
        }
      } catch (e) {
        if (!errorMessage(e).includes('stopped')) this.error = errorMessage(e)
      } finally {
        unlistenProgress()
        unlistenHeic()
        this.scanning = false
      }
    },

    loadFromHistory(entry) {
      if (this.scanning) return
      if (entry.id === this.activeHistoryEntryId) return

      this.folders = [...entry.folders]
      this.images = [...entry.images]
      this.searchQuery = ''
      this.selected = new Set()
      this.multiSelect = false
      this.networkFolders = new Set()
      this.locationNames = {}
      this.filterDateFrom = ''
      this.filterDateTo   = ''
      this.filterLocation = ''
      this.filterDevice   = ''
      this.error = null
      this.activeHistoryEntryId = entry.id
      this.scanDone = true
    },

    async stopScan() {
      try { await invoke('stop_meta_scan') } catch { /* ignore */ }
      if (_geocodeAbortController) {
        _geocodeAbortController.abort()
        _geocodeAbortController = null
      }
    },

    async geocodeAll() {
      // Deduplicate by ~1 km grid (2 decimal places ≈ 1.1 km)
      // so a trip with 300 photos in the same city = 1 API call.
      const groups = {}
      for (const img of this.images) {
        if (img.gpsLatitude == null || img.gpsLongitude == null) continue
        const key = `${img.gpsLatitude.toFixed(2)},${img.gpsLongitude.toFixed(2)}`
        if (!groups[key]) groups[key] = { lat: img.gpsLatitude, lon: img.gpsLongitude, paths: [] }
        groups[key].paths.push(img.path)
      }

      if (!Object.keys(groups).length) return

      // Fill from persistent cache first — skip any group already known
      const geoCache = await loadGeoCache()
      const pending = []
      for (const [key, group] of Object.entries(groups)) {
        if (key in geoCache) {
          for (const p of group.paths) this.locationNames[p] = geoCache[key]
          if (geoCache[key]) this.saveDiscoveredLocation(geoCache[key])
        } else {
          pending.push({ key, ...group })
        }
      }

      if (!pending.length) return

      const abort = new AbortController()
      _geocodeAbortController = abort
      this.geocoding = true
      try {
        for (const { key, lat, lon, paths } of pending) {
          if (abort.signal.aborted) break
          try {
            const res = await fetch(
              `https://nominatim.openstreetmap.org/reverse?lat=${lat}&lon=${lon}&format=json`,
              { headers: { 'User-Agent': 'RustyMirror/1.0 (desktop app)' }, signal: abort.signal }
            )
            const data = await res.json()
            const addr = data.address ?? {}
            const city = addr.city ?? addr.town ?? addr.village ?? addr.municipality ?? addr.county ?? null
            const name = [city, addr.country].filter(Boolean).join(', ') || ''
            for (const p of paths) this.locationNames[p] = name
            if (name) this.saveDiscoveredLocation(name)
            geoCache[key] = name
            const stats = geoCacheStats(geoCache)
            this.geoCacheCount = stats.count
            this.geoCacheBytes = stats.bytes
          } catch (e) {
            if (e.name === 'AbortError') break
            for (const p of paths) this.locationNames[p] = ''
          }
          if (abort.signal.aborted) break
          // Nominatim ToS: max 1 req/s
          await new Promise(r => setTimeout(r, 1100))
        }
      } finally {
        this.geocoding = false
        _geocodeAbortController = null
        await saveGeoCache(geoCache)
      }
    },

    async fetchLocationsManual() {
      if (this.geocodingManual || this.geocoding || this.scanning) return
      this.locationNames = {}
      this.geocodingManual = true
      try {
        await this.geocodeAll()
      } finally {
        this.geocodingManual = false
      }
    },

    async fetchDevicesManual() {
      if (this.devicesManual || this.scanning) return
      this.devicesManual = true
      try {
        const seen = new Set()
        for (const img of this.images) {
          if (img.device) seen.add(img.device)
        }
        this.discoveredDevices = [...seen].sort((a, b) =>
          a.localeCompare(b, undefined, { sensitivity: 'base', numeric: true }))
        const store = await getStore()
        await store.set(DISCOVERED_DEVICES_KEY, this.discoveredDevices)
      } catch { /* ignore */ } finally {
        this.devicesManual = false
      }
    },

    isNetworkPath(path) {
      for (const f of this.networkFolders) {
        if (path.startsWith(f)) return true
      }
      return false
    },

    async loadGeoCacheCount() {
      const cache = await loadGeoCache()
      const stats = geoCacheStats(cache)
      this.geoCacheCount = stats.count
      this.geoCacheBytes = stats.bytes
    },

    async clearGeoCache() {
      try {
        const store = await getStore()
        await store.set(GEO_CACHE_KEY, {})
      } catch { /* ignore */ }
      this.geoCacheCount = 0
      this.geoCacheBytes = 0
      this.locationNames = {}
    },

    async loadCustomLocations() {
      try {
        const store = await getStore()
        this.customLocations = (await store.get(CUSTOM_LOCATIONS_KEY)) ?? []
      } catch { this.customLocations = [] }
    },

    async loadDiscoveredLocations() {
      try {
        const store = await getStore()
        this.discoveredLocations = (await store.get(DISCOVERED_LOCATIONS_KEY)) ?? []
      } catch { this.discoveredLocations = [] }
    },

    async saveDiscoveredLocation(name) {
      if (!name || this.discoveredLocations.includes(name)) return
      this.discoveredLocations = [...this.discoveredLocations, name]
      try {
        const store = await getStore()
        await store.set(DISCOVERED_LOCATIONS_KEY, this.discoveredLocations)
      } catch {}
    },

    async addCustomLocation(name) {
      const trimmed = name.trim()
      if (!trimmed || this.customLocations.includes(trimmed)) return
      this.customLocations = [...this.customLocations, trimmed]
      try {
        const store = await getStore()
        await store.set(CUSTOM_LOCATIONS_KEY, this.customLocations)
      } catch {}
    },

    async removeCustomLocation(name) {
      this.customLocations = this.customLocations.filter(l => l !== name)
      try {
        const store = await getStore()
        await store.set(CUSTOM_LOCATIONS_KEY, this.customLocations)
      } catch {}
    },

    async removeLocation(name) {
      this.discoveredLocations = this.discoveredLocations.filter(l => l !== name)
      this.customLocations     = this.customLocations.filter(l => l !== name)
      if (this.filterLocation === name) this.filterLocation = ''

      // Remove from session locationNames
      const newNames = {}
      for (const [path, locName] of Object.entries(this.locationNames)) {
        if (locName !== name) newNames[path] = locName
      }
      this.locationNames = newNames

      // Remove matching entries from geo cache
      try {
        const store = await getStore()
        const geoCache = (await store.get(GEO_CACHE_KEY)) ?? {}
        let changed = false
        for (const [key, val] of Object.entries(geoCache)) {
          if (val === name) { delete geoCache[key]; changed = true }
        }
        if (changed) {
          const stats = geoCacheStats(geoCache)
          this.geoCacheCount = stats.count
          this.geoCacheBytes = stats.bytes
          await store.set(GEO_CACHE_KEY, geoCache)
        }
        await store.set(DISCOVERED_LOCATIONS_KEY, this.discoveredLocations)
        await store.set(CUSTOM_LOCATIONS_KEY, this.customLocations)
      } catch {}
    },

    async loadCustomDevices() {
      try {
        const store = await getStore()
        this.customDevices = (await store.get(CUSTOM_DEVICES_KEY)) ?? []
      } catch { this.customDevices = [] }
    },

    async loadDiscoveredDevices() {
      try {
        const store = await getStore()
        this.discoveredDevices = (await store.get(DISCOVERED_DEVICES_KEY)) ?? []
      } catch { this.discoveredDevices = [] }
    },

    async saveDiscoveredDevice(name) {
      if (!name || this.discoveredDevices.includes(name)) return
      this.discoveredDevices = [...this.discoveredDevices, name]
      try {
        const store = await getStore()
        await store.set(DISCOVERED_DEVICES_KEY, this.discoveredDevices)
      } catch {}
    },

    async addCustomDevice(name) {
      const trimmed = name.trim()
      if (!trimmed || this.customDevices.includes(trimmed)) return
      this.customDevices = [...this.customDevices, trimmed]
      try {
        const store = await getStore()
        await store.set(CUSTOM_DEVICES_KEY, this.customDevices)
      } catch {}
    },

    async removeCustomDevice(name) {
      this.customDevices = this.customDevices.filter(d => d !== name)
      try {
        const store = await getStore()
        await store.set(CUSTOM_DEVICES_KEY, this.customDevices)
      } catch {}
    },

    async loadDeviceAliases() {
      try {
        const store = await getStore()
        this.deviceAliases = (await store.get(DEVICE_ALIASES_KEY)) ?? {}
      } catch { this.deviceAliases = {} }
    },

    async setDeviceAlias(deviceName, alias) {
      const trimmed = alias.trim()
      const next = { ...this.deviceAliases }
      if (trimmed) {
        next[deviceName] = trimmed
      } else {
        delete next[deviceName]
      }
      this.deviceAliases = next
      try {
        const store = await getStore()
        await store.set(DEVICE_ALIASES_KEY, this.deviceAliases)
      } catch {}
    },

    async removeDevice(name) {
      this.discoveredDevices = this.discoveredDevices.filter(d => d !== name)
      this.customDevices     = this.customDevices.filter(d => d !== name)
      if (this.filterDevice === name) this.filterDevice = ''

      // Clear device field from images so availableDevices getter stops listing it
      this.images = this.images.map(img =>
        img.device === name ? { ...img, device: undefined } : img
      )

      // Remove alias if one exists
      if (this.deviceAliases[name]) {
        const next = { ...this.deviceAliases }
        delete next[name]
        this.deviceAliases = next
      }

      try {
        const store = await getStore()
        await store.set(DISCOVERED_DEVICES_KEY, this.discoveredDevices)
        await store.set(CUSTOM_DEVICES_KEY, this.customDevices)
        await store.set(DEVICE_ALIASES_KEY, this.deviceAliases)
      } catch {}
    },

    async deleteSelected() {
      const paths = [...this.selected]
      const pathSet = new Set(paths)
      await invoke('delete_files', { paths })
      this.images = this.images.filter(e => !pathSet.has(e.path))
      this.selected = new Set()
    },
  },
})
