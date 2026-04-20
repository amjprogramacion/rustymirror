import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

export const useOrganizerStore = defineStore('organizer', () => {
  const folders = ref([])

  const config = ref({
    onlyRename: true,
    datePriority: 'exif',        // 'exif' | 'filename'
    overrideYear: false,
    yearIfNotDate: 2015,
    outputDirectory: '',
  })

  const scanning      = ref(false)
  const scanResult    = ref(null)  // { total, images, videos } | null
  const sortBy        = ref('filename')  // 'filename' | 'type'
  const sortDir       = ref('asc')
  const previewing         = ref(false)
  const previewingDate     = ref(false)
  const executing          = ref(false)
  const executingOp        = ref(null)  // 'rename' | 'rewrite' | null
  const progress           = ref({ processed: 0, total: 0 })
  const previewActions     = ref([])   // OrganizerFileAction[]
  const previewDateActions = ref([])   // RewriteDateAction[]
  const lastSummary        = ref(null) // OrganizerSummary | null
  const error              = ref(null)

  let _unlisten = null

  // ── Folder management ────────────────────────────────────────────────────────

  function addFolder(path) {
    if (!folders.value.includes(path)) {
      folders.value.push(path)
      previewActions.value = []
      lastSummary.value    = null
    }
  }

  function removeFolder(path) {
    folders.value = folders.value.filter(f => f !== path)
    previewActions.value = []
    lastSummary.value    = null
  }

  function updateConfig(partial) {
    Object.assign(config.value, partial)
  }

  // ── Progress listener ────────────────────────────────────────────────────────

  async function _subscribeProgress() {
    if (_unlisten) { _unlisten(); _unlisten = null }
    _unlisten = await listen('organize_progress', ({ payload }) => {
      progress.value = payload
    })
  }

  function _unsubscribeProgress() {
    if (_unlisten) { _unlisten(); _unlisten = null }
  }

  function _buildConfig() {
    return {
      onlyRename:      config.value.onlyRename,
      datePriority:    config.value.datePriority,
      overrideYear:    config.value.overrideYear,
      yearIfNotDate:   config.value.yearIfNotDate,
      outputDirectory: config.value.outputDirectory,
    }
  }

  // ── Actions ──────────────────────────────────────────────────────────────────

  async function runScan() {
    if (!folders.value.length) return
    scanning.value  = true
    error.value     = null
    scanResult.value = null
    try {
      scanResult.value = await invoke('count_media_files', { paths: folders.value, config: _buildConfig() })
    } catch (e) {
      error.value = e?.message ?? String(e)
    } finally {
      scanning.value = false
    }
  }

  async function runPreviewRewrite() {
    if (!folders.value.length) return
    previewingDate.value     = true
    progress.value           = { processed: 0, total: 0 }
    error.value              = null
    previewDateActions.value = []
    lastSummary.value        = null

    await _subscribeProgress()
    try {
      previewDateActions.value = await invoke('preview_rewrite_date', {
        paths:  folders.value,
        config: _buildConfig(),
      })
    } catch (e) {
      error.value = e?.message ?? String(e)
    } finally {
      previewingDate.value = false
      _unsubscribeProgress()
    }
  }

  async function runPreview() {
    if (!folders.value.length) return
    previewing.value     = true
    progress.value       = { processed: 0, total: 0 }
    error.value          = null
    previewActions.value = []
    lastSummary.value    = null

    await _subscribeProgress()
    try {
      previewActions.value = await invoke('preview_organize', {
        paths:  folders.value,
        config: _buildConfig(),
      })
    } catch (e) {
      error.value = e?.message ?? String(e)
    } finally {
      previewing.value = false
      _unsubscribeProgress()
    }
  }

  async function runExecute() {
    if (!folders.value.length) return
    executing.value   = true
    executingOp.value = 'rename'
    progress.value    = { processed: 0, total: 0 }
    error.value       = null
    lastSummary.value = null

    await _subscribeProgress()
    try {
      lastSummary.value    = await invoke('execute_organize', {
        paths:  folders.value,
        config: _buildConfig(),
      })
      previewActions.value = []
    } catch (e) {
      error.value = e?.message ?? String(e)
    } finally {
      executing.value   = false
      executingOp.value = null
      _unsubscribeProgress()
    }
    if (!error.value) await runScan()
  }

  async function runMetadataRewrite() {
    if (!folders.value.length) return
    executing.value   = true
    executingOp.value = 'rewrite'
    progress.value    = { processed: 0, total: 0 }
    error.value       = null
    lastSummary.value = null

    await _subscribeProgress()
    try {
      lastSummary.value = await invoke('execute_metadata_rewrite', {
        paths:  folders.value,
        config: _buildConfig(),
      })
    } catch (e) {
      error.value = e?.message ?? String(e)
    } finally {
      executing.value   = false
      executingOp.value = null
      _unsubscribeProgress()
    }
    if (!error.value) await runScan()
  }

  async function stop() {
    await invoke('stop_organize').catch(() => {})
  }

  return {
    folders, config,
    scanning, scanResult, sortBy, sortDir,
    previewing, previewingDate, executing, executingOp, progress,
    previewActions, previewDateActions, lastSummary, error,
    addFolder, removeFolder, updateConfig,
    runScan, runPreviewRewrite, runPreview, runExecute, runMetadataRewrite, stop,
  }
})
