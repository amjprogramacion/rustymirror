import { defineStore } from 'pinia'
import { invoke, convertFileSrc } from '@tauri-apps/api/core'
import { useSettings } from '../composables/useSettings'

export const useThumbnailStore = defineStore('thumbnails', {
  state: () => ({
    thumbCache: {},
    directSrcCache: {},
    _thumbQueue: [],
    _thumbActive: 0,
    heicThumbGenerated: 0,
    networkFolders: new Set(),
  }),
  actions: {
    clearCache() {
      this.thumbCache = {}
      this.directSrcCache = {}
      this._thumbQueue = []
      this._thumbActive = 0
      this.heicThumbGenerated = 0
    },
    setNetworkFolders(folders) {
      this.networkFolders = new Set(folders)
    },
    isNetworkPath(path) {
      for (const f of this.networkFolders) {
        if (path.startsWith(f)) return true
      }
      return false
    },
    setThumb(path, src)     { this.thumbCache[path] = src },
    setDirectSrc(path, src) { this.directSrcCache[path] = src },
    clearThumbQueue()       { this._thumbQueue = [] },
    dequeueThumbnail(path) {
      const idx = this._thumbQueue.indexOf(path)
      if (idx !== -1) this._thumbQueue.splice(idx, 1)
    },
    enqueueThumbnail(path) {
      if (path in this.thumbCache) return
      if (this.directSrcCache[path]) return
      if (this._thumbQueue.includes(path)) return
      this._thumbQueue.push(path)
      this._flushThumbQueue()
    },
    _flushThumbQueue() {
      const MAX = useSettings().thumbConcurrency.value
      while (this._thumbActive < MAX && this._thumbQueue.length > 0) {
        const path = this._thumbQueue.shift()
        if (path in this.thumbCache) continue
        if (this.directSrcCache[path]) continue
        this._thumbActive++
        invoke('get_thumbnail', { path })
          .then(src => {
            this.thumbCache[path] = src
            this.heicThumbGenerated++
          })
          .catch(() => {
            const ext = path.split('.').pop()?.toLowerCase() ?? ''
            const rustOnly = ext === 'heic' || ext === 'heif' || ext === 'png'
            if (!rustOnly && !this.isNetworkPath(path)) {
              this.directSrcCache[path] = convertFileSrc(path)
            } else {
              this.thumbCache[path] = '__error__'
            }
          })
          .finally(() => {
            this._thumbActive--
            this._flushThumbQueue()
          })
      }
    },
  },
})
