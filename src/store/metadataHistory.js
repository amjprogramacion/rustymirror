import { createHistoryStore, foldersKey } from './historyFactory'
import { isVideo } from '../utils/formatters'

// imageCount on a metadata history entry counts ALL media (collect_media bundles
// images + videos), so the video tally is derived from the snapshot paths.
function countVideos(images) {
  return images?.reduce((n, im) => n + (isVideo(im.path) ? 1 : 0), 0) ?? 0
}

// Bump this whenever the shape or data source of ImageEntry fields changes
// (e.g. switching metadata backend, adding new fields). Cached entries with a
// different version are treated as stale and a fresh scan is forced.
const CACHE_VERSION = 3

// Coalesces rapid snapshot edits (e.g. a batch save patching N images) into a
// single persist instead of one disk write per image.
let _saveTimer = null

export const useMetadataHistoryStore = createHistoryStore({
  id: 'metadataHistory',
  historyKey: 'metaScanHistory',
  logPrefix: 'metadataHistory',
  clearLabel: 'metadata history',

  // Each entry: { id, folders, date, durationMs, imageCount, videoCount, fingerprint, images }
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
      videoCount: countVideos(images),
      fingerprint: fingerprint ?? null,
      images: images ?? [],
      _v: CACHE_VERSION,
    }

    this._upsert(match, entry)
    await this._save()
    return entry.id
  },

  extraActions: {
    // Patch a single image inside a stored snapshot so reloading that history
    // entry reflects an edit made after the scan (e.g. a GPS location set).
    updateImage(entryId, path, newEntry) {
      const entry = this.entries.find(e => e.id === entryId)
      if (!entry?.images) return
      const idx = entry.images.findIndex(im => im.path === path)
      if (idx === -1) return
      entry.images[idx] = newEntry
      clearTimeout(_saveTimer)
      _saveTimer = setTimeout(() => this._save(), 300)
    },

    // Drop deleted images from a stored snapshot so reloading the entry from
    // history doesn't resurface files that no longer exist. The fingerprint is
    // cleared because the directory changed (forces a fresh scan if re-scanned).
    async removeImages(entryId, paths) {
      const entry = this.entries.find(e => e.id === entryId)
      if (!entry?.images) return
      const set = new Set(paths)
      entry.images = entry.images.filter(im => !set.has(im.path))
      entry.imageCount = entry.images.length
      entry.videoCount = countVideos(entry.images)
      entry.fingerprint = null
      await this._save()
    },

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
