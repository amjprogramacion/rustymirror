import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'

export const useMetadataStore = defineStore('metadata', {
  state: () => ({
    folders: [],
    scanning: false,
    scanDone: false,
    images: [],
    searchQuery: '',
    error: null,
    multiSelect: false,
    selected: new Set(),
    networkFolders: new Set(),
  }),

  getters: {
    filteredImages(state) {
      const q = state.searchQuery.trim().toLowerCase()
      if (!q) return state.images
      return state.images.filter(e => {
        const name = e.path.split(/[/\\]/).pop() ?? ''
        return name.toLowerCase().includes(q)
      })
    },

    selectedCount(state) {
      return state.selected.size
    },
  },

  actions: {
    addFolder(path) {
      if (!this.folders.includes(path)) this.folders.push(path)
    },
    removeFolder(path) {
      this.folders = this.folders.filter(f => f !== path)
    },

    toggleSelected(path) {
      if (this.selected.has(path)) this.selected.delete(path)
      else this.selected.add(path)
    },
    clearSelection() {
      this.selected = new Set()
    },

    async startScan() {
      this.scanning = true
      this.scanDone = false
      this.images = []
      this.searchQuery = ''
      this.selected = new Set()
      this.multiSelect = false
      this.networkFolders = new Set()
      this.error = null

      try {
        const images = await invoke('scan_for_metadata', { paths: this.folders })
        this.images = images
        this.scanDone = true

        try {
          const checks = await Promise.all(this.folders.map(f => invoke('is_network_path', { path: f })))
          this.networkFolders = new Set(this.folders.filter((_, i) => checks[i]))
        } catch { /* ignore */ }
      } catch (e) {
        this.error = String(e)
      } finally {
        this.scanning = false
      }
    },

    isNetworkPath(path) {
      for (const f of this.networkFolders) {
        if (path.startsWith(f)) return true
      }
      return false
    },
  },
})
