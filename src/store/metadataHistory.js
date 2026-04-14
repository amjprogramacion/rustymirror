import { defineStore } from 'pinia'
import { load } from '@tauri-apps/plugin-store'
import { HISTORY_MAX_ENTRIES } from '../constants'
import { logger } from '../utils/logger'

const STORE_FILE = 'rustymirror.json'
const HISTORY_KEY = 'metaScanHistory'

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

    // Returns cached images only if folders and fingerprint both match.
    getCached(folders, fingerprint) {
      const key = foldersKey(folders)
      const entry = this.entries.find(e => foldersKey(e.folders) === key)
      if (!entry || !entry.fingerprint || !entry.images?.length) return null
      if (entry.fingerprint !== fingerprint) return null
      return entry.images
    },

    async clearHistory() {
      const count = this.entries.length
      this.entries = []
      await this._save()
      logger.info(`metadata history cleared count=${count} key=${HISTORY_KEY}`)
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
