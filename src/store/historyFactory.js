import { defineStore } from 'pinia'
import { load } from '@tauri-apps/plugin-store'
import { HISTORY_MAX_ENTRIES } from '../constants'
import { logger } from '../utils/logger'

const STORE_FILE = 'rustymirror.json'

let _store = null
async function getStore() {
  if (!_store) _store = await load(STORE_FILE, { autoSave: true })
  return _store
}

export function foldersKey(folders) {
  return [...folders].sort().join('|')
}

/**
 * Build a Pinia store for a persisted, folder-keyed scan history.
 *
 * @param {object}   opts
 * @param {string}   opts.id              Pinia store id.
 * @param {string}   opts.historyKey      Key used inside the shared tauri-plugin-store file.
 * @param {string}   opts.logPrefix       Used in `[prefix] load/save failed` warnings.
 * @param {string}   opts.clearLabel      Label used when logging `clearHistory`.
 * @param {Function} opts.addEntry        Custom addEntry action (has access to `this`).
 * @param {object}   [opts.extraActions]  Extra actions (e.g. getCached) bound to the store.
 */
export function createHistoryStore({ id, historyKey, logPrefix, clearLabel, addEntry, extraActions = {} }) {
  return defineStore(id, {
    state: () => ({
      entries: [],
      folderStatus: {},
      missingFolders: {},
    }),

    actions: {
      async load() {
        try {
          const store = await getStore()
          const saved = await store.get(historyKey)
          if (Array.isArray(saved)) this.entries = saved
        } catch (e) {
          console.warn(`[${logPrefix}] load failed:`, e)
        }
      },

      addEntry,

      async removeEntry(entryId) {
        this.entries = this.entries.filter(e => e.id !== entryId)
        delete this.folderStatus[entryId]
        await this._save()
      },

      async clearHistory() {
        const count = this.entries.length
        this.entries = []
        await this._save()
        logger.info(`${clearLabel} cleared count=${count} key=${historyKey}`)
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

      _upsert(matchFn, entry) {
        const existingIdx = this.entries.findIndex(matchFn)
        if (existingIdx >= 0) {
          this.entries[existingIdx] = entry
        } else {
          this.entries.unshift(entry)
          if (this.entries.length > HISTORY_MAX_ENTRIES) {
            this.entries = this.entries.slice(0, HISTORY_MAX_ENTRIES)
          }
        }
      },

      async _save() {
        try {
          const store = await getStore()
          await store.set(historyKey, this.entries)
        } catch (e) {
          console.warn(`[${logPrefix}] save failed:`, e)
        }
      },

      ...extraActions,
    },
  })
}
