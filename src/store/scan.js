import { defineStore } from 'pinia'
import { invoke, convertFileSrc } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { logger } from '../utils/logger'
import { useHistoryStore } from './history'
import { useMetadataStore } from './metadata'
import { useSettings } from '../composables/useSettings'

export const useScanStore = defineStore('scan', {
  state: () => ({
    folders: [],
    scanning: false,
    scanLabel: '',
    progress: { scanned: 0, total: 0 },
    analyzeProgress: { analyzed: 0, total: 0, phase: '' },
    scanStartTime: null,
    _etaSamples: [],
    groups: [],
    filter: 'all',
    extFilter: '',
    searchQuery: '',
    similarityThreshold: 90,
    multiSelect: false,
    selected: new Set(),
    scanDone: false,
    error: null,
    thumbCache: {},
    networkFolders: new Set(),
    lightbox: null,
    directSrcCache: {},
    // Metadata panel: null when closed, { entry, metadata, loading, saving, error, dirty } when open
    metadataPanel: null,
    metadataPanelHeight: 300,
    heicThumbGenerated: 0,
    _thumbQueue: [],
    _thumbActive: 0,
    // ID of the history entry currently displayed in the results panel.
    // Set after each completed scan; used by Sidebar to highlight the active entry.
    activeHistoryEntryId: null,
    fingerprinting: false,
    // When set (by loadFromHistory), overrides the localStorage fast mode setting
    // for exactly one scan. Consumed and reset to null at the start of startScan.
    fastModeOverride: null,
    // When set (by loadFromHistory), overrides the localStorage cross-date phash setting
    // for exactly one scan. Consumed and reset to null at the start of startScan.
    crossDatePhashOverride: null,
  }),

  getters: {
    filteredGroups(state) {
      // Helper: is this group a sameDate with a similarity score?
      const isSameDateSimilar = g => g.kind === 'sameDate' && g.similarity != null
      // Helper: is this group a sameDate without a similarity score?
      const isSameDateOnly   = g => g.kind === 'sameDate' && g.similarity == null

      // ── Filter by kind ──────────────────────────────────────────────────────
      // 'similar' filter shows: similar + sameDate-with-similarity
      // 'sameDate' filter shows: only sameDate-without-similarity
      // 'exact'   filter shows: only exact
      // 'all'     filter shows: everything
      let filtered
      if (state.filter === 'all') {
        filtered = [...state.groups]
      } else if (state.filter === 'exact') {
        filtered = state.groups.filter(g => g.kind === 'exact')
      } else if (state.filter === 'similar') {
        filtered = state.groups.filter(g => g.kind === 'similar' || isSameDateSimilar(g))
      } else if (state.filter === 'sameDate') {
        filtered = state.groups.filter(g => isSameDateOnly(g))
      } else {
        filtered = state.groups.filter(g => g.kind === state.filter)
      }

      // ── Extension filter ────────────────────────────────────────────────────
      if (state.extFilter) {
        const ext = state.extFilter.toLowerCase()
        filtered = filtered.filter(g =>
          g.entries.some(e => (e.path.split('.').pop()?.toLowerCase() ?? '') === ext)
        )
      }

      // ── Search ──────────────────────────────────────────────────────────────
      const q = state.searchQuery.trim().toLowerCase()
      if (q) {
        filtered = filtered.filter(g =>
          g.entries.some(e => {
            const name = e.path.split(/[/\\]/).pop() ?? ''
            return name.toLowerCase().includes(q)
          })
        )
      }

      // ── Sort ────────────────────────────────────────────────────────────────
      // Order:
      //   1. exact
      //   2. similar (desc by similarity%)
      //   3. sameDate with similarity (desc by similarity%)
      //   4. sameDate without similarity
      // Ties within any bucket: oldest entry date ascending
      const sortKey = g => {
        if (g.kind === 'exact')              return 0
        if (g.kind === 'similar')            return 1
        if (isSameDateSimilar(g))            return 2
        return 3 // sameDate without similarity
      }

      filtered.sort((a, b) => {
        const ka = sortKey(a)
        const kb = sortKey(b)
        if (ka !== kb) return ka - kb

        // Within buckets 1 and 2 (similar / sameDate-similar): desc by similarity
        if ((ka === 1 || ka === 2) && a.similarity != null && b.similarity != null) {
          const simDiff = (b.similarity ?? 0) - (a.similarity ?? 0)
          if (simDiff !== 0) return simDiff
        }

        // Tiebreak: oldest entry date ascending
        const aDate = a.entries[0]?.modified ?? ''
        const bDate = b.entries[0]?.modified ?? ''
        return aDate < bDate ? -1 : aDate > bDate ? 1 : 0
      })

      // ── Within-group entry order ─────────────────────────────────────────────
      // Original first, then copies:
      //   - exact groups  → alphabetical by filename ascending (case-insensitive)
      //   - other groups  → by date ascending
      return filtered.map(g => ({
        ...g,
        entries: [
          ...g.entries.filter(e => e.isOriginal),
          ...[...g.entries.filter(e => !e.isOriginal)]
            .sort((a, b) => {
              if (g.kind === 'exact') {
                const nameA = a.path.split(/[/\\]/).pop() ?? ''
                const nameB = b.path.split(/[/\\]/).pop() ?? ''
                return nameA.localeCompare(nameB, undefined, { sensitivity: 'base' })
              }
              return a.modified < b.modified ? -1 : a.modified > b.modified ? 1 : 0
            })
        ]
      }))
    },

    groupCounts(state) {
      const isSameDateSimilar = g => g.kind === 'sameDate' && g.similarity != null
      const isSameDateOnly    = g => g.kind === 'sameDate' && g.similarity == null

      const counts = {
        all:      state.groups.length,
        exact:    0,
        similar:  0,
        sameDate: 0,
      }
      for (const g of state.groups) {
        if (g.kind === 'exact')        counts.exact++
        else if (g.kind === 'similar') counts.similar++
        else if (isSameDateSimilar(g)) counts.similar++  // counted under similar filter
        else if (isSameDateOnly(g))    counts.sameDate++
      }
      return counts
    },

    // Sorted list of unique file extensions present across all current groups.
    availableExtensions(state) {
      const exts = new Set()
      for (const g of state.groups) {
        for (const e of g.entries) {
          const ext = e.path.split('.').pop()?.toLowerCase()
          if (ext) exts.add(ext)
        }
      }
      return [...exts].sort()
    },

    selectedCount(state) {
      return state.selected.size
    },
    progressPercent(state) {
      if (!state.progress.total) return 0
      return Math.round((state.progress.scanned / state.progress.total) * 100)
    },
    // ETA using a 10-second rolling window to handle variable processing speed
    etaSeconds(state) {
      const samples = state._etaSamples
      if (samples.length < 2) return null
      const newest = samples[samples.length - 1]
      const oldest = samples[0]
      const windowSecs = (newest.time - oldest.time) / 1000
      if (windowSecs < 1) return null
      const processedInWindow = newest.scanned - oldest.scanned
      if (processedInWindow <= 0) return null
      const rate = processedInWindow / windowSecs  // images/sec in window
      const remaining = state.progress.total - state.progress.scanned
      if (remaining <= 0) return null
      const eta = Math.round(remaining / rate)
      // Only show ETA if it's meaningful (more than 2s and less than 1h)
      if (eta < 2 || eta > 3600) return null
      return eta
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
    selectCopies() {
      this.selected = new Set(
        this.groups
          .flatMap(g => g.entries)
          .filter(e => !e.isOriginal)
          .map(e => e.path)
      )
    },
    clearSelection() {
      this.selected = new Set()
    },

    async startScan() {
      this.scanning = true
      this.scanDone = false
      this.groups = []
      this.filter = 'all'
      this.extFilter = ''
      this.selected = new Set()
      this.searchQuery = ''
      this._thumbQueue = []
      this._thumbActive = 0
      this.heicThumbGenerated = 0
      this.networkFolders = new Set()
      this.error = null
      this.progress = { scanned: 0, total: 0 }
      this.analyzeProgress = { analyzed: 0, total: 0, phase: '' }
      this.scanStartTime = Date.now()
      this._scanCancelled = false
      this._etaSamples = []
      logger.info(`scan started — ${this.folders.length} folder(s): ${this.folders.join(', ')}`)

      const unlisten = await listen('scan_progress', (event) => {
        this.progress = event.payload
        const now = Date.now()
        this._etaSamples.push({ scanned: event.payload.scanned, time: now })
        const cutoff = now - 10000
        this._etaSamples = this._etaSamples.filter(s => s.time >= cutoff)
      })
      const unlistenAnalyze = await listen('analyze_progress', (event) => {
        this.analyzeProgress = event.payload
      })

      try {
        const history = useHistoryStore()
        const hammingThreshold = Math.round((100 - this.similarityThreshold) / 100 * 64)

        // Resolve fastMode and crossDatePhash before the try block so they are visible
        // in all branches below. Uses one-shot overrides set by loadFromHistory, or
        // falls back to the current settings.
        const settings = useSettings()
        const fastMode = this.fastModeOverride !== null
          ? this.fastModeOverride
          : settings.fastMode.value
        this.fastModeOverride = null
        const crossDatePhash = this.crossDatePhashOverride !== null
          ? this.crossDatePhashOverride
          : settings.crossDatePhash.value
        this.crossDatePhashOverride = null

        logger.info('computing directory fingerprint...')
        let groups = null
        let fingerprint = null
        let fromCache = false
        try {
          this.fingerprinting = true
          fingerprint = await invoke('directory_fingerprint', { paths: this.folders })
          this.fingerprinting = false
          logger.info(`fingerprint: ${fingerprint.slice(0, 12)}...`)
          const cached = history.getCached(this.folders, fingerprint, this.similarityThreshold, fastMode, crossDatePhash)
          if (cached) {
            logger.info(`cache hit — skipping scan (${cached.length} groups)`)
            groups = cached
            fromCache = true
          }
        } catch (fpErr) {
          logger.warn('fingerprint failed, proceeding with full scan')
        }

        if (!groups) {
          logger.info('invoking scan_directories...')
          logger.info(`similarity threshold: ${this.similarityThreshold}% -> hamming ${hammingThreshold}`)
          groups = await invoke('scan_directories', { paths: this.folders, hammingThreshold, crossDatePhash, fastMode })
          logger.info(`scan returned ${groups?.length} groups`)
        }

        logger.info(`scan complete: ${groups.length} group(s) found`)

        // -1 = cancelled, null = cache hit (keep original), positive = real duration.
        const durationMs = fromCache ? null
          : this._scanCancelled ? -1
          : this.scanStartTime ? Date.now() - this.scanStartTime
          : null

        if (!this._scanCancelled) {
          const imageCount = groups.reduce((n, g) => n + g.entries.length, 0)
          const entryId = await history.addEntry(this.folders, groups.length, imageCount, groups, fingerprint, this.similarityThreshold, fastMode, crossDatePhash, durationMs)

          this.groups = groups
          this.scanDone = true
          this.activeHistoryEntryId = entryId ?? null

          try {
            const checks = await Promise.all(this.folders.map(f => invoke('is_network_path', { path: f })))
            this.setNetworkFolders(this.folders.filter((_, i) => checks[i]))
          } catch { /* ignore */ }
        }
      } catch (e) {
        logger.error('scan failed:', String(e))
        this.error = String(e)
      } finally {
        this.scanning = false
        this.fingerprinting = false
        this.scanLabel = ''
        this.scanStartTime = null
        unlisten()
        unlistenAnalyze()
      }
    },

    async stopScan() {
      logger.warn('scan stopped by user')
      this._scanCancelled = true
      this.scanLabel = 'Stopping…'
      await invoke('stop_scan')
      // Do NOT set scanning = false here — the startScan finally block handles it.
      // This prevents starting a new scan while Rust is still winding down.
    },

    openLightbox(entries, index) {
      this.lightbox = { entries, index }
    },
    closeLightbox() {
      this.lightbox = null
    },

    async openMetadataPanel(entry) {
      this.metadataPanel = { entry, metadata: null, loading: true, saving: false, error: null, dirty: false }
      try {
        const metadata = await invoke('read_metadata', { path: entry.path })
        if (this.metadataPanel) this.metadataPanel = { ...this.metadataPanel, metadata, loading: false }
      } catch (e) {
        if (this.metadataPanel) this.metadataPanel = { ...this.metadataPanel, loading: false, error: String(e) }
      }
    },
    async openBatchEditPanel(entries) {
      this.metadataPanel = { batch: true, entries, allMetadata: null, loading: true, saving: false, error: null, dirty: false }
      try {
        const allMetadata = await Promise.all(entries.map(e => invoke('read_metadata', { path: e.path })))
        if (this.metadataPanel) this.metadataPanel = { ...this.metadataPanel, allMetadata, loading: false }
      } catch (e) {
        if (this.metadataPanel) this.metadataPanel = { ...this.metadataPanel, loading: false, error: String(e) }
      }
    },
    closeMetadataPanel() {
      this.metadataPanel = null
    },
    // Patch the matching entry inside groups[] after a metadata save
    updateGroupEntry(path, metadata) {
      const d      = new Date()
      const pad    = n => String(n).padStart(2, '0')
      const now    = `${d.getFullYear()}-${pad(d.getMonth()+1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
      const device = [metadata.make, metadata.model].filter(Boolean).join(' ') || undefined
      for (const group of this.groups) {
        const idx = group.entries.findIndex(e => e.path === path)
        if (idx !== -1) {
          group.entries.splice(idx, 1, {
            ...group.entries[idx],
            modified:     now,
            dateTaken:    metadata.dateTimeOriginal ?? group.entries[idx].dateTaken,
            gpsLatitude:  metadata.gpsLatitude  ?? null,
            gpsLongitude: metadata.gpsLongitude ?? null,
            device:       device ?? group.entries[idx].device,
          })
          break
        }
      }
    },
    async saveMetadata(update) {
      if (!this.metadataPanel) return
      this.metadataPanel = { ...this.metadataPanel, saving: true, error: null }
      try {
        const path = this.metadataPanel.entry.path
        await invoke('write_metadata', { path, update })
        const metadata = await invoke('read_metadata', { path })
        this.metadataPanel = { ...this.metadataPanel, metadata, saving: false, dirty: false }
        useMetadataStore().updateEntryFromMetadata(path, metadata)
        this.updateGroupEntry(path, metadata)
      } catch (e) {
        this.metadataPanel = { ...this.metadataPanel, saving: false, error: String(e) }
      }
    },
    async saveBatchMetadata(update) {
      if (!this.metadataPanel?.batch) return
      this.metadataPanel = { ...this.metadataPanel, saving: true, error: null }
      try {
        await Promise.all(this.metadataPanel.entries.map(e => invoke('write_metadata', { path: e.path, update })))
        const allMetadata = await Promise.all(this.metadataPanel.entries.map(e => invoke('read_metadata', { path: e.path })))
        if (this.metadataPanel) {
          this.metadataPanel = { ...this.metadataPanel, allMetadata, saving: false, dirty: false }
          const metaStore = useMetadataStore()
          this.metadataPanel.entries.forEach((e, i) => {
            metaStore.updateEntryFromMetadata(e.path, allMetadata[i])
            this.updateGroupEntry(e.path, allMetadata[i])
          })
        }
      } catch (e) {
        if (this.metadataPanel) this.metadataPanel = { ...this.metadataPanel, saving: false, error: String(e) }
      }
    },
    lightboxNext() {
      if (!this.lightbox) return
      this.lightbox.index = (this.lightbox.index + 1) % this.lightbox.entries.length
    },
    lightboxPrev() {
      if (!this.lightbox) return
      this.lightbox.index = (this.lightbox.index - 1 + this.lightbox.entries.length) % this.lightbox.entries.length
    },
    setThumb(path, src) {
      this.thumbCache[path] = src
    },
    setDirectSrc(path, src) {
      this.directSrcCache[path] = src
    },

    setNetworkFolders(folders) {
      this.networkFolders = new Set(folders)
    },

    isNetworkPath(path) {
      for (const f of this.networkFolders) {
        if (path.startsWith(f)) return true
      }
      return false
    },

    // Drop all pending (not yet in-flight) thumbnail requests
    clearThumbQueue() {
      this._thumbQueue = []
    },

    // Remove a single path from the pending queue (if not yet in-flight)
    dequeueThumbnail(path) {
      const idx = this._thumbQueue.indexOf(path)
      if (idx !== -1) this._thumbQueue.splice(idx, 1)
    },

    // Enqueue a thumbnail load, respecting global concurrency (max 4 at once)
    enqueueThumbnail(path) {
      if (path in this.thumbCache) return
      if (this.directSrcCache[path]) return   // already has a working direct URL
      if (this._thumbQueue.includes(path)) return
      this._thumbQueue.push(path)
      this._flushThumbQueue()
    },

    _flushThumbQueue() {
      const MAX = useSettings().thumbConcurrency.value
      while (this._thumbActive < MAX && this._thumbQueue.length > 0) {
        const path = this._thumbQueue.shift()
        if (path in this.thumbCache) continue
        if (this.directSrcCache[path]) continue  // already resolved via convertFileSrc
        this._thumbActive++
        invoke('get_thumbnail', { path })
          .then(src => {
            this.thumbCache[path] = src
            this.heicThumbGenerated++
          })
          .catch((e) => {
            console.warn('Thumbnail generation failed:', path, e)
            // PNGs and HEIC are always routed through Rust — don't fall back to
            // convertFileSrc for them (that's exactly what we're trying to avoid).
            // For other formats on local paths, let the browser try natively.
            const ext = path.split('.').pop()?.toLowerCase() ?? ''
            const rustOnly = ext === 'heic' || ext === 'heif' || ext === 'png'
            if (!rustOnly && !this.isNetworkPath(path)) {
              this.directSrcCache[path] = convertFileSrc(path)
            } else {
              this.thumbCache[path] = '__error__'
            }
          })
          .finally(() => {
            this._thumbActive--
            this._flushThumbQueue()
          })
      }
    },

    async deleteSelected() {
      const paths = [...this.selected]
      const pathSet = new Set(paths)
      logger.info(`deleting ${paths.length} files`)
      try {
        await invoke('delete_files', { paths })
        logger.info('delete_files returned ok')
      } catch (e) {
        logger.error(`delete_files failed: ${e}`)
        throw e
      }

      const updated = this.groups
        .map(g => {
          const remaining = g.entries.filter(e => !pathSet.has(e.path))
          if (remaining.length < 2) return null
          const hasOriginal = remaining.some(e => e.isOriginal)
          if (!hasOriginal) {
            // Fix: use size_bytes (snake_case) as it comes from Rust serialization
            const best = [...remaining].sort((a, b) =>
              (b.size_bytes - a.size_bytes) || (b.width * b.height - a.width * a.height)
            )[0]
            remaining.forEach(e => { e.isOriginal = e.path === best.path })
          }
          return { ...g, entries: remaining }
        })
        .filter(Boolean)

      this.groups = updated
      this.selected = new Set()
      logger.info(`groups after delete: ${this.groups.length}`)

      const history = useHistoryStore()
      const histEntry = history.entries.find(e => e.id === this.activeHistoryEntryId)
      if (histEntry) {
        histEntry.fingerprint = null
        histEntry.groups = this.groups
        await history._save()
      }
    },
  },
})
