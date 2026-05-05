import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { logger } from '../utils/logger'
import { errorMessage } from '../utils/errors'
import { fileName, fileExt } from '../utils/formatters'
import { useDuplicatesHistoryStore } from './duplicatesHistory'
import { useMetadataStore } from './metadata'
import { useSettings } from '../composables/useSettings'
import { useThumbnailStore } from './thumbnails'
import { usePanelStore } from './panel'

export const useDuplicatesStore = defineStore('duplicates', {
  state: () => ({
    folders: [],
    scanning: false,
    stopping: false,
    scanLabel: '',
    progress: { scanned: 0, total: 0 },
    analyzeProgress: { analyzed: 0, total: 0, phase: '' },
    scanStartTime: null,
    _etaSamples: [],
    groups: [],
    failedFiles: [],
    filter: 'all',
    extFilter: '',
    dupSortBy: 'group',
    dupSortDir: 'desc',
    searchQuery: '',
    similarityThreshold: 90,
    multiSelect: false,
    selected: new Set(),
    scanDone: false,
    error: null,
    networkFolders: new Set(),
    lightbox: null,
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
    // Retention rule: which copy to treat as the original in each group.
    // Serialises as { kind: 'highestResolution' | 'oldestDate' | 'newestDate' |
    //                 'highestSharpness' | 'filenamePattern', pattern?: string }
    retentionRule: { kind: 'highestResolution' },
  }),

  getters: {
    // Each stage is a separate computed so Vue caches it independently.
    // Changing only searchQuery skips the kind/ext stages; changing only
    // dupSortBy skips all filter stages, etc.

    // Stage 1 — filter by group kind
    _groupsByKind(state) {
      const isSameDateSimilar = g => g.kind === 'sameDate' && g.similarity != null
      const isSameDateOnly    = g => g.kind === 'sameDate' && g.similarity == null
      if (state.filter === 'all')      return state.groups
      if (state.filter === 'exact')    return state.groups.filter(g => g.kind === 'exact')
      if (state.filter === 'similar')  return state.groups.filter(g => g.kind === 'similar' || isSameDateSimilar(g))
      if (state.filter === 'sameDate') return state.groups.filter(g => isSameDateOnly(g))
      return state.groups.filter(g => g.kind === state.filter)
    },

    // Stage 2 — filter by file extension
    _groupsByExt(state) {
      if (!state.extFilter) return this._groupsByKind
      const ext = state.extFilter.toLowerCase()
      return this._groupsByKind.filter(g =>
        g.entries.some(e => fileExt(e.path) === ext)
      )
    },

    // Stage 3 — filter by search query
    _groupsBySearch(state) {
      const q = state.searchQuery.trim().toLowerCase()
      if (!q) return this._groupsByExt
      return this._groupsByExt.filter(g =>
        g.entries.some(e => fileName(e.path).toLowerCase().includes(q))
      )
    },

    // Stage 4 — sort
    _groupsSorted(state) {
      const isSameDateSimilar = g => g.kind === 'sameDate' && g.similarity != null
      const sortKey = g => {
        if (g.kind === 'exact')   return 0
        if (g.kind === 'similar') return 1
        if (isSameDateSimilar(g)) return 2
        return 3
      }
      const arr = [...this._groupsBySearch]
      if (state.dupSortBy === 'group') {
        // desc: exact → similar → sameDate-similar → sameDate
        // asc:  sameDate → sameDate-similar → similar → exact
        const dir = state.dupSortDir === 'asc' ? -1 : 1
        arr.sort((a, b) => {
          const ka = sortKey(a)
          const kb = sortKey(b)
          if (ka !== kb) return (ka - kb) * dir
          if ((ka === 1 || ka === 2) && a.similarity != null && b.similarity != null) {
            return (b.similarity ?? 0) - (a.similarity ?? 0)
          }
          return 0
        })
      } else {
        // Date or title: pure sort, kind order is ignored
        const dir = state.dupSortDir === 'desc' ? -1 : 1
        arr.sort((a, b) => {
          if (state.dupSortBy === 'title') {
            const aName = fileName(a.entries[0]?.path).toLowerCase()
            const bName = fileName(b.entries[0]?.path).toLowerCase()
            return aName < bName ? -dir : aName > bName ? dir : 0
          }
          // date: dateTaken, fall back to modified
          const aDate = a.entries[0]?.dateTaken || a.entries[0]?.modified || ''
          const bDate = b.entries[0]?.dateTaken || b.entries[0]?.modified || ''
          return aDate < bDate ? -dir : aDate > bDate ? dir : 0
        })
      }
      return arr
    },

    // Stage 5 — reorder entries within each group (original first, then copies)
    //   - exact groups  → alphabetical by filename ascending (case-insensitive)
    //   - other groups  → by date ascending
    filteredGroups() {
      return this._groupsSorted.map(g => ({
        ...g,
        entries: [
          ...g.entries.filter(e => e.isOriginal),
          ...[...g.entries.filter(e => !e.isOriginal)]
            .sort((a, b) => {
              if (g.kind === 'exact') {
                return fileName(a.path).localeCompare(fileName(b.path), undefined, { sensitivity: 'base' })
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
          const ext = fileExt(e.path)
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

    async applyRetentionRule(rule) {
      if (!this.groups.length) return
      this.retentionRule = rule
      try {
        this.groups = await invoke('apply_retention_rule_cmd', { groups: this.groups, rule })
      } catch (e) {
        logger.warn('apply_retention_rule_cmd failed:', errorMessage(e))
      }
    },
    clearSelection() {
      this.selected = new Set()
    },

    async startScan() {
      this.scanning = true
      this.scanDone = false
      this.groups = []
      this.failedFiles = []
      this.filter = 'all'
      this.extFilter = ''
      this.dupSortBy = 'group'
      this.dupSortDir = 'desc'
      this.selected = new Set()
      this.searchQuery = ''
      useThumbnailStore().clearCache()
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
        const history = useDuplicatesHistoryStore()
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
          const result = await invoke('scan_directories', { paths: this.folders, hammingThreshold, crossDatePhash, fastMode, retentionRule: this.retentionRule })
          groups = result.groups
          this.failedFiles = result.failedFiles || []
          logger.info(`scan returned ${groups?.length} groups, ${this.failedFiles.length} failed files`)
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
            const networkFolders = this.folders.filter((_, i) => checks[i])
            this.setNetworkFolders(networkFolders)
            useThumbnailStore().setNetworkFolders(networkFolders)
          } catch { /* ignore */ }
        }
      } catch (e) {
        logger.error('scan failed:', errorMessage(e))
        this.error = errorMessage(e)
      } finally {
        this.scanning = false
        this.stopping = false
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
      this.stopping = true
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

    // Patch the matching entry inside groups[] after a metadata save
    updateGroupEntry(path, metadata) {
      const d      = new Date()
      const pad    = n => String(n).padStart(2, '0')
      const now    = `${d.getFullYear()}-${pad(d.getMonth()+1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
      const device = [metadata.make, metadata.model].filter(Boolean).join(' ') || null
      for (const group of this.groups) {
        const idx = group.entries.findIndex(e => e.path === path)
        if (idx !== -1) {
          group.entries.splice(idx, 1, {
            ...group.entries[idx],
            modified:     now,
            dateTaken:    metadata.dateTimeOriginal ?? group.entries[idx].dateTaken,
            gpsLatitude:  metadata.gpsLatitude  ?? null,
            gpsLongitude: metadata.gpsLongitude ?? null,
            device,
          })
          break
        }
      }
    },
    async saveMetadata(update) {
      const panel = usePanelStore()
      if (!panel.activePanel) return
      panel.activePanel = { ...panel.activePanel, saving: true, error: null }
      try {
        const path = panel.activePanel.entry.path
        await invoke('write_metadata', { path, update })
        const metadata = await invoke('read_metadata', { path })
        panel.activePanel = { ...panel.activePanel, metadata, saving: false, dirty: false }
        useMetadataStore().updateEntryFromMetadata(path, metadata)
        this.updateGroupEntry(path, metadata)
      } catch (e) {
        panel.activePanel = { ...panel.activePanel, saving: false, error: errorMessage(e) }
      }
    },
    async saveBatchMetadata(update) {
      const panel = usePanelStore()
      if (!panel.activePanel?.batch) return
      panel.activePanel = { ...panel.activePanel, saving: true, error: null }
      try {
        await Promise.all(panel.activePanel.entries.map(e => invoke('write_metadata', { path: e.path, update })))
        const allMetadata = await Promise.all(panel.activePanel.entries.map(e => invoke('read_metadata', { path: e.path })))
        if (panel.activePanel) {
          panel.activePanel = { ...panel.activePanel, allMetadata, saving: false, dirty: false }
          const metaStore = useMetadataStore()
          panel.activePanel.entries.forEach((e, i) => {
            metaStore.updateEntryFromMetadata(e.path, allMetadata[i])
            this.updateGroupEntry(e.path, allMetadata[i])
          })
        }
      } catch (e) {
        if (panel.activePanel) panel.activePanel = { ...panel.activePanel, saving: false, error: errorMessage(e) }
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
    setNetworkFolders(folders) {
      this.networkFolders = new Set(folders)
    },

    isNetworkPath(path) {
      for (const f of this.networkFolders) {
        if (path.startsWith(f)) return true
      }
      return false
    },

    async deleteSelected() {
      const paths = [...this.selected]
      const pathSet = new Set(paths)
      logger.info(`deleting ${paths.length} files`)
      try {
        await invoke('delete_files', { paths })
        logger.info('delete_files returned ok')
      } catch (e) {
        logger.error(`delete_files failed: ${errorMessage(e)}`)
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

      const history = useDuplicatesHistoryStore()
      const histEntry = history.entries.find(e => e.id === this.activeHistoryEntryId)
      if (histEntry) {
        histEntry.fingerprint = null
        histEntry.groups = this.groups
        await history._save()
      }
    },
  },
})
