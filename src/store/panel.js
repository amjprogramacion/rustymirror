import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'

export const usePanelStore = defineStore('panel', {
  state: () => ({
    // null when closed.
    // Single: { entry, metadata, loading, saving, error, dirty }
    // Batch:  { batch: true, entries, allMetadata, loading, saving, error, dirty }
    activePanel: null,
    panelHeight: 300,
  }),
  actions: {
    async openPanel(entry) {
      this.activePanel = { entry, metadata: null, loading: true, saving: false, error: null, dirty: false }
      try {
        const metadata = await invoke('read_metadata', { path: entry.path })
        if (this.activePanel) this.activePanel = { ...this.activePanel, metadata, loading: false }
      } catch (e) {
        if (this.activePanel) this.activePanel = { ...this.activePanel, loading: false, error: String(e) }
      }
    },
    async openBatchPanel(entries) {
      this.activePanel = { batch: true, entries, allMetadata: null, loading: true, saving: false, error: null, dirty: false }
      try {
        const allMetadata = await Promise.all(entries.map(e => invoke('read_metadata', { path: e.path })))
        if (this.activePanel) this.activePanel = { ...this.activePanel, allMetadata, loading: false }
      } catch (e) {
        if (this.activePanel) this.activePanel = { ...this.activePanel, loading: false, error: String(e) }
      }
    },
    closePanel() {
      this.activePanel = null
    },
  },
})
