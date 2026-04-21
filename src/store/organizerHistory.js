import { defineStore } from 'pinia'
import { load } from '@tauri-apps/plugin-store'
import { HISTORY_MAX_ENTRIES } from '../constants'
import { logger } from '../utils/logger'

const STORE_FILE = 'rustymirror.json'
const HISTORY_KEY = 'organizerScanHistory'

let _store = null

async function getStore() {
  if (!_store) _store = await load(STORE_FILE, { autoSave: true })
  return _store
}

function foldersKey(folders) {
  return [...folders].sort().join('|')
}

export const useOrganizerHistoryStore = defineStore('organizerHistory', {
  state: () => ({
    // Each entry: { id, folders, date, durationMs, total, images, videos }
    entries: [],
    // { [id]: 'ok' | 'partial' | 'missing' } — checked on load, not persisted
    folderStatus: {},
  }),

  actions: {
    async load() {
      try {
        const store = await getStore()
        const saved = await store.get(HISTORY_KEY)
        if (Array.isArray(saved)) this.entries = saved
      } catch (e) {
        console.warn('[organizerHistory] load failed:', e)
      }
    },

    async addEntry(folders, total, images, videos, durationMs) {
      const key = foldersKey(folders)
      const existing = this.entries.find(e => foldersKey(e.folders) === key)

      const entry = {
        id: existing?.id ?? Date.now(),
        folders: [...folders],
        date: existing?.date ?? new Date().toISOString(),
        durationMs: existing?.durationMs ?? durationMs ?? null,
        total,
        images,
        videos,
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

    async removeEntry(id) {
      this.entries = this.entries.filter(e => e.id !== id)
      delete this.folderStatus[id]
      await this._save()
    },

    async clearHistory() {
      const count = this.entries.length
      this.entries = []
      await this._save()
      logger.info(`organizer history cleared count=${count} key=${HISTORY_KEY}`)
    },

    async checkFolderStatus() {
      const { exists } = await import('@tauri-apps/plugin-fs')
      for (const entry of this.entries) {
        const results = await Promise.all(entry.folders.map(f => exists(f).catch(() => false)))
        const missing = results.filter(r => !r).length
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
        console.warn('[organizerHistory] save failed:', e)
      }
    },
  },
})
