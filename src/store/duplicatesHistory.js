import { defineStore } from 'pinia'
import { load } from '@tauri-apps/plugin-store'
import { HISTORY_MAX_ENTRIES } from '../constants'
import { logger } from '../utils/logger'

const STORE_FILE = 'rustymirror.json'
const HISTORY_KEY = 'scanHistory'

let _store = null

async function getStore() {
  if (!_store) _store = await load(STORE_FILE, { autoSave: true })
  return _store
}

function foldersKey(folders) {
  return [...folders].sort().join('|')
}

// Cache key includes threshold, fastMode and crossDatePhash so that each combination
// produces a separate history entry (different options may return different groups).
function cacheKey(folders, threshold, fastMode, crossDatePhash) {
  return `${foldersKey(folders)}@@${threshold}@@${fastMode ? 'fast' : 'precise'}@@${crossDatePhash ? 'cross' : 'nocross'}`
}

export const useDuplicatesHistoryStore = defineStore('duplicatesHistory', {
  state: () => ({
    // Each entry: { id, folders, date, duplicates, imageCount, fingerprint, groups, threshold }
    entries: [],
    // { [id]: 'ok' | 'partial' | 'missing' } — checked on load, not persisted
    folderStatus: {},
    // { [id]: string[] } — paths that no longer exist, not persisted
    missingFolders: {},
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

    async addEntry(folders, duplicates, imageCount, groups, fingerprint, threshold, fastMode, crossDatePhash, durationMs) {
      // Preserve original date, id and duration when the same scan is re-run (cache hit).
      const key = cacheKey(folders, threshold, fastMode, crossDatePhash)
      const existing = this.entries.find(e => cacheKey(e.folders, e.threshold ?? 90, e.fastMode ?? false, e.crossDatePhash ?? true) === key)

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

      const existingIdx = this.entries.findIndex(
        e => cacheKey(e.folders, e.threshold ?? 90, e.fastMode ?? false, e.crossDatePhash ?? true) === key
      )
      if (existingIdx >= 0) {
        // Update in-place — preserves original position in the list.
        this.entries[existingIdx] = entry
      } else {
        this.entries.unshift(entry)
        if (this.entries.length > HISTORY_MAX_ENTRIES) {
          this.entries = this.entries.slice(0, HISTORY_MAX_ENTRIES)
        }
      }

      await this._save()
      return entry.id
    },

    // Returns cached groups only if folders, fingerprint, threshold, fastMode AND crossDatePhash all match.
    getCached(folders, fingerprint, threshold, fastMode, crossDatePhash) {
      const key = cacheKey(folders, threshold, fastMode, crossDatePhash)
      const entry = this.entries.find(e => cacheKey(e.folders, e.threshold ?? 90, e.fastMode ?? false, e.crossDatePhash ?? true) === key)
      if (!entry || !entry.fingerprint || !entry.groups?.length) return null
      if (entry.fingerprint !== fingerprint) return null
      return entry.groups
    },

    async removeEntry(id) {
      this.entries = this.entries.filter(e => e.id !== id)
      delete this.folderStatus[id]
      await this._save()
    },

    async clearHistory() {
      const count = this.entries.length
      this.entries = []
      await this._save()
      logger.info(`scan history cleared count=${count} key=${HISTORY_KEY}`)
    },

    async checkFolderStatus() {
      const { invoke } = await import('@tauri-apps/api/core')
      for (const entry of this.entries) {
        const results = await invoke('check_paths_exist', { paths: entry.folders }).catch(() => entry.folders.map(() => false))
        const missing = results.filter(r => !r).length
        this.missingFolders[entry.id] = entry.folders.filter((_, i) => !results[i])
        if (missing === 0) {
          this.folderStatus[entry.id] = 'ok'
        } else if (entry.folders.length === 1) {
          this.folderStatus[entry.id] = 'missing'
        } else {
          this.folderStatus[entry.id] = 'partial'
        }
      }
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
