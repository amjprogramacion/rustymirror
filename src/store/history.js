import { defineStore } from 'pinia'
import { load } from '@tauri-apps/plugin-store'

const STORE_FILE = 'rustymirror.json'
const HISTORY_KEY = 'scanHistory'
const MAX_ENTRIES = 5

let _store = null

async function getStore() {
  if (!_store) _store = await load(STORE_FILE, { autoSave: true })
  return _store
}

function foldersKey(folders) {
  return [...folders].sort().join('|')
}

// Cache key includes threshold and fastMode so that each combination produces
// a separate history entry (fast and precise scans may return different groups).
function cacheKey(folders, threshold, fastMode) {
  return `${foldersKey(folders)}@@${threshold}@@${fastMode ? 'fast' : 'precise'}`
}

export const useHistoryStore = defineStore('history', {
  state: () => ({
    // Each entry: { id, folders, date, duplicates, imageCount, fingerprint, groups, threshold }
    entries: [],
  }),

  actions: {
    async load() {
      try {
        const store = await getStore()
        const saved = await store.get(HISTORY_KEY)
        if (Array.isArray(saved)) this.entries = saved
      } catch (e) {
        console.warn('[history] load failed:', e)
      }
    },

    async addEntry(folders, duplicates, imageCount, groups, fingerprint, threshold, fastMode, durationMs) {
      // Preserve original date, id and duration when the same scan is re-run (cache hit).
      const key = cacheKey(folders, threshold, fastMode)
      const existing = this.entries.find(e => cacheKey(e.folders, e.threshold ?? 90, e.fastMode ?? false) === key)

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
      }

      const existingIdx = this.entries.findIndex(
        e => cacheKey(e.folders, e.threshold ?? 90, e.fastMode ?? false) === key
      )
      if (existingIdx >= 0) {
        // Update in-place — preserves original position in the list.
        this.entries[existingIdx] = entry
      } else {
        this.entries.unshift(entry)
        if (this.entries.length > MAX_ENTRIES) {
          this.entries = this.entries.slice(0, MAX_ENTRIES)
        }
      }

      await this._save()
      return entry.id
    },

    // Returns cached groups only if folders, fingerprint, threshold AND fastMode all match.
    getCached(folders, fingerprint, threshold, fastMode) {
      const key = cacheKey(folders, threshold, fastMode)
      const entry = this.entries.find(e => cacheKey(e.folders, e.threshold ?? 90, e.fastMode ?? false) === key)
      if (!entry || !entry.fingerprint || !entry.groups?.length) return null
      if (entry.fingerprint !== fingerprint) return null
      return entry.groups
    },

    async clearHistory() {
      this.entries = []
      await this._save()
    },

    async _save() {
      try {
        const store = await getStore()
        await store.set(HISTORY_KEY, this.entries)
      } catch (e) {
        console.warn('[history] save failed:', e)
      }
    },
  },
})
