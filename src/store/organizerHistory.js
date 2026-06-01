import { createHistoryStore, foldersKey } from './historyFactory'

// Bump whenever the cached scan-result shape or its data source changes, so
// older entries are treated as stale and re-scanned instead of restored.
const CACHE_VERSION = 1

// Rebuilds the scanResult object the organizer store expects from a stored entry.
function toScanResult(entry) {
  return {
    total:     entry.total,
    images:    entry.images,
    videos:    entry.videos,
    imageExts: entry.imageExts ?? {},
    videoExts: entry.videoExts ?? {},
    files:     entry.files ?? [],
  }
}

export const useOrganizerHistoryStore = createHistoryStore({
  id: 'organizerHistory',
  historyKey: 'organizerScanHistory',
  logPrefix: 'organizerHistory',
  clearLabel: 'organizer history',

  // Each entry caches the full scan result so it can be restored without a
  // re-scan. `files` carries per-file EXIF dates, which depend on the date
  // priority config — stored as `datePriority` so a config change invalidates it.
  // Entry: { id, folders, date, durationMs, total, images, videos,
  //          imageExts, videoExts, files, datePriority, fingerprint, _v }
  async addEntry(folders, result, fingerprint, durationMs, datePriority) {
    const key = foldersKey(folders)
    const match = e => foldersKey(e.folders) === key
    const existing = this.entries.find(match)

    const entry = {
      id: existing?.id ?? Date.now(),
      folders: [...folders],
      // Never update the date — it records when the scan was FIRST run.
      date: existing?.date ?? new Date().toISOString(),
      // Never update duration once set — it records how long the FIRST real scan took.
      durationMs: existing?.durationMs ?? durationMs ?? null,
      total:     result.total,
      images:    result.images,
      videos:    result.videos,
      imageExts: result.imageExts ?? {},
      videoExts: result.videoExts ?? {},
      files:     result.files ?? [],
      datePriority: [...(datePriority ?? [])],
      fingerprint: fingerprint ?? null,
      _v: CACHE_VERSION,
    }

    this._upsert(match, entry)
    await this._save()
    return entry.id
  },

  extraActions: {
    // Cache hit on Scan: returns the stored result only if folders, fingerprint,
    // schema version, and the date-priority config (order matters) all match.
    getCached(folders, fingerprint, datePriority) {
      const key = foldersKey(folders)
      const entry = this.entries.find(e => foldersKey(e.folders) === key)
      if (!entry || !entry.fingerprint || !entry.files?.length) return null
      if (entry.fingerprint !== fingerprint) return null
      if (entry._v !== CACHE_VERSION) return null
      if ((entry.datePriority ?? []).join('|') !== (datePriority ?? []).join('|')) return null
      return toScanResult(entry)
    },

    // Direct restore when clicking a history entry — trusts the stored snapshot
    // without re-validating the fingerprint, mirroring the metadata tool.
    getEntryResult(entryId) {
      const entry = this.entries.find(e => e.id === entryId)
      if (!entry?.files?.length) return null
      return toScanResult(entry)
    },

    // Strips the heavy cached scan data (file lists) from every entry while
    // keeping the history list itself. Next load/scan re-reads from disk.
    async clearCache() {
      let changed = false
      for (const e of this.entries) {
        if (e.files?.length || e.fingerprint) {
          e.files = []
          e.imageExts = {}
          e.videoExts = {}
          e.fingerprint = null
          changed = true
        }
      }
      if (changed) await this._save()
    },
  },
})
