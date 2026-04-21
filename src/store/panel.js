import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { errorMessage } from '../utils/errors'
import { useMetadataStore } from './metadata'

export const usePanelStore = defineStore('panel', {
  state: () => ({
    // null when closed.
    // Single: { entry, metadata, loading, saving, error, dirty }
    // Batch:  { batch: true, entries, allMetadata, loading, saving, error, dirty }
    activePanel: null,
    panelHeight: 300,
  }),
  actions: {
    openPanel(entry) {
      this.activePanel = { entry, metadata: null, loading: true, saving: false, error: null, dirty: false }
      Promise.resolve().then(() => this._fetchMetadata(entry.path))
    },
    async _fetchMetadata(path) {
      try {
        const metadata = await invoke('read_metadata', { path })
        if (this.activePanel && !this.activePanel.batch && this.activePanel.entry?.path === path) {
          this.activePanel = { ...this.activePanel, metadata, loading: false }
        }
        const device = [metadata.make, metadata.model].filter(Boolean).join(' ')
        if (device) useMetadataStore().saveDiscoveredDevice(device)
      } catch (e) {
        if (this.activePanel && !this.activePanel.batch && this.activePanel.entry?.path === path) {
          this.activePanel = { ...this.activePanel, loading: false, error: errorMessage(e) }
        }
      }
    },
    openBatchPanel(entries) {
      this.activePanel = { batch: true, entries, allMetadata: null, loading: true, saving: false, error: null, dirty: false }
      Promise.resolve().then(() => this._fetchBatchMetadata(entries))
    },
    async _fetchBatchMetadata(entries) {
      try {
        const allMetadata = await Promise.all(entries.map(e => invoke('read_metadata', { path: e.path })))
        if (this.activePanel?.batch) {
          this.activePanel = { ...this.activePanel, allMetadata, loading: false }
        }
        const metaStore = useMetadataStore()
        for (const metadata of allMetadata) {
          const device = [metadata.make, metadata.model].filter(Boolean).join(' ')
          if (device) metaStore.saveDiscoveredDevice(device)
        }
      } catch (e) {
        if (this.activePanel?.batch) {
          this.activePanel = { ...this.activePanel, loading: false, error: errorMessage(e) }
        }
      }
    },
    closePanel() {
      this.activePanel = null
    },
  },
})
