import { defineStore } from 'pinia'
import { load } from '@tauri-apps/plugin-store'
import { HISTORY_MAX_ENTRIES } from '../constants'
import { logger } from '../utils/logger'

const STORE_FILE = 'rustymirror.json'
const HISTORY_KEY = 'metaScanHistory'
// Bump this whenever the shape or data source of ImageEntry fields changes
// (e.g. switching metadata backend, adding new fields). Cached entries with a
// different version are treated as stale and a fresh scan is forced.
const CACHE_VERSION = 3

let _store = null

async function getStore() {
  if (!_store) _store = await load(STORE_FILE, { autoSave: true })
  return _store
}

function foldersKey(folders) {
  return [...folders].sort().join('|')
}

export const useMetadataHistoryStore = defineStore('metadataHistory', {
  state: () => ({
    // Each entry: { id, folders, date, durationMs, imageCount, fingerprint, images }
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
        console.warn('[metadataHistory] load failed:', e)
      }
    },

    async addEntry(folders, imageCount, images, fingerprint, durationMs) {
      const key = foldersKey(folders)
      const existing = this.entries.find(e => foldersKey(e.folders) === key)

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

      const existingIdx = this.entries.findIndex(e => foldersKey(e.folders) === key)
      if (existingIdx >= 0) {
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

    // Returns cached images only if folders, fingerprint, and schema version all match.
    getCached(folders, fingerprint) {
      const key = foldersKey(folders)
      const entry = this.entries.find(e => foldersKey(e.folders) === key)
      if (!entry || !entry.fingerprint || !entry.images?.length) return null
      if (entry.fingerprint !== fingerprint) return null
      if (entry._v !== CACHE_VERSION) return null
      return entry.images
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
      logger.info(`metadata history cleared count=${count} key=${HISTORY_KEY}`)
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
        console.warn('[metadataHistory] save failed:', e)
      }
    },
  },
})
