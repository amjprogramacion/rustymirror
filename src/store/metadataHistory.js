import { createHistoryStore, foldersKey } from './historyFactory'

// Bump this whenever the shape or data source of ImageEntry fields changes
// (e.g. switching metadata backend, adding new fields). Cached entries with a
// different version are treated as stale and a fresh scan is forced.
const CACHE_VERSION = 3

export const useMetadataHistoryStore = createHistoryStore({
  id: 'metadataHistory',
  historyKey: 'metaScanHistory',
  logPrefix: 'metadataHistory',
  clearLabel: 'metadata history',

  // Each entry: { id, folders, date, durationMs, imageCount, fingerprint, images }
  async addEntry(folders, imageCount, images, fingerprint, durationMs) {
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
      imageCount,
      fingerprint: fingerprint ?? null,
      images: images ?? [],
      _v: CACHE_VERSION,
    }

    this._upsert(match, entry)
    await this._save()
    return entry.id
  },

  extraActions: {
    // Returns cached images only if folders, fingerprint, and schema version all match.
    getCached(folders, fingerprint) {
      const key = foldersKey(folders)
      const entry = this.entries.find(e => foldersKey(e.folders) === key)
      if (!entry || !entry.fingerprint || !entry.images?.length) return null
      if (entry.fingerprint !== fingerprint) return null
      if (entry._v !== CACHE_VERSION) return null
      return entry.images
    },
  },
})
