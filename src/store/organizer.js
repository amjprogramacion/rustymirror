import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useOrganizerHistoryStore } from './organizerHistory'

export const useOrganizerStore = defineStore('organizer', () => {
  const folders = ref([])

  const config = ref({
    onlyRename: true,
    datePriority: 'exif',        // 'exif' | 'filename'
    overrideYear: false,
    yearIfNotDate: 2015,
    outputDirectory: '',
    renameTemplate: '{type}_{date}_{time}_{4hex_uid}',
    folderTemplate: 'REORDENADAS/{year}/{device}/{month_dir}',
  })

  const scanning            = ref(false)
  const scanResult          = ref(null)  // { total, images, videos } | null
  const activeHistoryEntryId = ref(null)
  const sortBy        = ref('filename')  // 'filename' | 'type'
  const sortDir       = ref('asc')
  const previewing         = ref(false)
  const previewingDate     = ref(false)
  const previewOnlyRename  = ref(null)  // onlyRename value at preview time, null = no preview
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
      renameTemplate:  config.value.renameTemplate,
      folderTemplate:  config.value.folderTemplate,
    }
  }

  // ── Actions ──────────────────────────────────────────────────────────────────

  async function runScan() {
    if (!folders.value.length) return
    scanning.value   = true
    error.value      = null
    scanResult.value = null
    const t0 = Date.now()
    try {
      const result = await invoke('count_media_files', { paths: folders.value, config: _buildConfig() })
      scanResult.value = result
      const orgHistory = useOrganizerHistoryStore()
      const entryId = await orgHistory.addEntry(
        folders.value, result.total, result.images, result.videos, Date.now() - t0
      )
      activeHistoryEntryId.value = entryId
    } catch (e) {
      error.value = e?.message ?? String(e)
    } finally {
      scanning.value = false
    }
  }

  async function loadFromHistory(entry) {
    if (scanning.value) return
    folders.value = [...entry.folders]
    if (entry.id === activeHistoryEntryId.value) return
    await runScan()
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
    previewing.value      = true
    previewOnlyRename.value = null
    progress.value        = { processed: 0, total: 0 }
    error.value           = null
    previewActions.value  = []
    lastSummary.value     = null

    const onlyRenameAtStart = config.value.onlyRename
    await _subscribeProgress()
    try {
      previewActions.value = await invoke('preview_organize', {
        paths:  folders.value,
        config: _buildConfig(),
      })
      previewOnlyRename.value = onlyRenameAtStart
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
      previewActions.value  = []
      previewOnlyRename.value = null
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
    scanning, scanResult, sortBy, sortDir, activeHistoryEntryId,
    previewing, previewingDate, executing, executingOp, progress,
    previewActions, previewDateActions, previewOnlyRename, lastSummary, error,
    addFolder, removeFolder, updateConfig,
    runScan, runPreviewRewrite, runPreview, runExecute, runMetadataRewrite, stop, loadFromHistory,
  }
})
