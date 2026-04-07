import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'

export const useMetadataStore = defineStore('metadata', {
  state: () => ({
    folders: [],
    scanning: false,
    scanDone: false,
    images: [],
    searchQuery: '',
    sortBy:  'date',  // 'date' | 'location' | 'device'
    sortDir: 'asc',   // 'asc'  | 'desc'
    locationNames: {}, // path → "City, Country" (populated in background after scan)
    geocoding: false,
    filterDateFrom: '',   // 'YYYY-MM-DD' or ''
    filterDateTo:   '',
    filterLocation: '',   // exact location name or '' = all
    filterDevice:   '',   // exact device string  or '' = all
    error: null,
    multiSelect: false,
    selected: new Set(),
    networkFolders: new Set(),
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
          const name = e.path.split(/[/\\]/).pop() ?? ''
          if (!name.toLowerCase().includes(q)) return false
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

    toggleSelected(path) {
      if (this.selected.has(path)) this.selected.delete(path)
      else this.selected.add(path)
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

      try {
        const images = await invoke('scan_for_metadata', { paths: this.folders })
        this.images = images
        this.scanDone = true

        try {
          const checks = await Promise.all(this.folders.map(f => invoke('is_network_path', { path: f })))
          this.networkFolders = new Set(this.folders.filter((_, i) => checks[i]))
        } catch { /* ignore */ }

        // Geocode in background — does not block the UI
        this.geocodeAll()
      } catch (e) {
        this.error = String(e)
      } finally {
        this.scanning = false
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

      const pending = Object.values(groups)
      if (!pending.length) return

      this.geocoding = true
      try {
        for (const { lat, lon, paths } of pending) {
          try {
            const res = await fetch(
              `https://nominatim.openstreetmap.org/reverse?lat=${lat}&lon=${lon}&format=json`,
              { headers: { 'User-Agent': 'RustyMirror/1.0 (desktop app)' } }
            )
            const data = await res.json()
            const addr = data.address ?? {}
            const city = addr.city ?? addr.town ?? addr.village ?? addr.municipality ?? addr.county ?? null
            const name = [city, addr.country].filter(Boolean).join(', ') || ''
            for (const p of paths) this.locationNames[p] = name
          } catch {
            for (const p of paths) this.locationNames[p] = ''
          }
          // Nominatim ToS: max 1 req/s
          await new Promise(r => setTimeout(r, 1100))
        }
      } finally {
        this.geocoding = false
      }
    },

    isNetworkPath(path) {
      for (const f of this.networkFolders) {
        if (path.startsWith(f)) return true
      }
      return false
    },
  },
})
