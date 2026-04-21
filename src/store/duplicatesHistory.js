import { createHistoryStore, foldersKey } from './historyFactory'

// Cache key includes threshold, fastMode and crossDatePhash so that each combination
// produces a separate history entry (different options may return different groups).
function cacheKey(folders, threshold, fastMode, crossDatePhash) {
  return `${foldersKey(folders)}@@${threshold}@@${fastMode ? 'fast' : 'precise'}@@${crossDatePhash ? 'cross' : 'nocross'}`
}

function entryCacheKey(e) {
  return cacheKey(e.folders, e.threshold ?? 90, e.fastMode ?? false, e.crossDatePhash ?? true)
}

export const useDuplicatesHistoryStore = createHistoryStore({
  id: 'duplicatesHistory',
  historyKey: 'scanHistory',
  logPrefix: 'history',
  clearLabel: 'scan history',

  // Each entry: { id, folders, date, duplicates, imageCount, fingerprint, groups, threshold }
  async addEntry(folders, duplicates, imageCount, groups, fingerprint, threshold, fastMode, crossDatePhash, durationMs) {
    const key = cacheKey(folders, threshold, fastMode, crossDatePhash)
    const match = e => entryCacheKey(e) === key
    // Preserve original date, id and duration when the same scan is re-run (cache hit).
    const existing = this.entries.find(match)

    const entry = {
      // Keep original id so activeHistoryEntryId remains valid across cache hits.
      id: existing?.id ?? Date.now(),
      folders: [...folders],
      // Never update the date — it records when the scan was FIRST run.
      date: existing?.date ?? new Date().toISOString(),
      // Never update duration once set — it records how long the FIRST real scan took.
      durationMs: existing?.durationMs ?? durationMs ?? null,
      duplicates,
      imageCount,
      fingerprint: fingerprint ?? null,
      groups: groups ?? [],
      threshold: threshold ?? 90,
      fastMode: fastMode ?? false,
      crossDatePhash: crossDatePhash ?? true,
    }

    this._upsert(match, entry)
    await this._save()
    return entry.id
  },

  extraActions: {
    // Returns cached groups only if folders, fingerprint, threshold, fastMode AND crossDatePhash all match.
    getCached(folders, fingerprint, threshold, fastMode, crossDatePhash) {
      const key = cacheKey(folders, threshold, fastMode, crossDatePhash)
      const entry = this.entries.find(e => entryCacheKey(e) === key)
      if (!entry || !entry.fingerprint || !entry.groups?.length) return null
      if (entry.fingerprint !== fingerprint) return null
      return entry.groups
    },
  },
})
