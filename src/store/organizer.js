import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useOrganizerHistoryStore } from './organizerHistory'
import { useMetadataStore } from './metadata'

export const useOrganizerStore = defineStore('organizer', () => {
  const folders = ref([])

  const config = ref({
    onlyRename: true,
    datePriority: 'exif',        // 'exif' | 'filename'
    overrideYear: false,
    yearIfNotDate: 2015,
    outputDirectory: '',
    renameTemplate: '{type}_{date}_{time}_{4hex_uid}',
    folderTemplate: '{year}/{device}/{month_dir}',
  })

  const scanning            = ref(false)
  const stopping            = ref(false)
  const scanResult          = ref(null)  // { total, images, videos, files } | null
  const scanProgress        = ref({ scanned: 0, total: 0 })
  const activeHistoryEntryId = ref(null)
  const sortBy        = ref('filename')  // 'filename' | 'date' | 'type'
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
  let _unlistenScan = null

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

  async function _subscribeScanProgress() {
    if (_unlistenScan) { _unlistenScan(); _unlistenScan = null }
    _unlistenScan = await listen('media_scan_progress', ({ payload }) => {
      scanProgress.value = payload
    })
  }

  function _unsubscribeScanProgress() {
    if (_unlistenScan) { _unlistenScan(); _unlistenScan = null }
  }

  function _buildConfig() {
    const metaStore = useMetadataStore()
    return {
      onlyRename:      config.value.onlyRename,
      datePriority:    config.value.datePriority,
      overrideYear:    config.value.overrideYear,
      yearIfNotDate:   config.value.yearIfNotDate,
      outputDirectory: config.value.outputDirectory,
      renameTemplate:  config.value.renameTemplate,
      folderTemplate:  config.value.folderTemplate,
      deviceAliases:   metaStore.deviceAliases,
    }
  }

  const VIDEO_EXTS = new Set(['mp4', 'mov', 'avi', 'mpg', 'mpeg', 'mkv'])

  function _fileType(name) {
    return VIDEO_EXTS.has(name.split('.').pop()?.toLowerCase() ?? '') ? 'video' : 'image'
  }

  // Returns file paths sorted exactly as currently displayed in the UI table.
  // Mirrors the sortedFiles computed in OrganizerView so the backend processes
  // files in the same order the user sees them.
  function _getSortedFilesList() {
    const files = scanResult.value?.files ?? []
    const dir = sortDir.value === 'asc' ? 1 : -1

    const sorted = [...files].sort((a, b) => {
      if (sortBy.value === 'type') {
        const typeA = _fileType(a.name)
        const typeB = _fileType(b.name)
        if (typeA !== typeB) return typeA.localeCompare(typeB) * dir
      }
      if (sortBy.value === 'date') {
        const da = a.dateTaken ?? ''
        const db = b.dateTaken ?? ''
        if (da !== db) return da.localeCompare(db) * dir
      }
      return a.name.toLowerCase().localeCompare(b.name.toLowerCase()) * dir
    })

    return sorted.map(f => f.path)
  }

  // ── Actions ──────────────────────────────────────────────────────────────────

  async function runScan() {
    if (!folders.value.length) return
    scanning.value       = true
    scanProgress.value   = { scanned: 0, total: 0 }
    error.value          = null
    scanResult.value     = null
    const t0 = Date.now()
    await _subscribeScanProgress()
    try {
      const result = await invoke('count_media_files', { paths: folders.value, config: _buildConfig() })
      scanResult.value = result
      const orgHistory = useOrganizerHistoryStore()
      const entryId = await orgHistory.addEntry(
        folders.value, result.total, result.images, result.videos, Date.now() - t0
      )
      activeHistoryEntryId.value = entryId
    } catch (e) {
      if (e?.type !== 'cancelled') error.value = e?.message ?? String(e)
    } finally {
      scanning.value = false
      stopping.value = false
      _unsubscribeScanProgress()
    }
  }

  async function loadFromHistory(entry) {
    if (scanning.value) return
    folders.value = [...entry.folders]
    if (entry.id === activeHistoryEntryId.value) return
    await runScan()
  }

  async function runPreviewRewrite() {
    if (!scanResult.value?.files?.length) return
    previewingDate.value     = true
    progress.value           = { processed: 0, total: 0 }
    error.value              = null
    previewDateActions.value = []
    lastSummary.value        = null

    await _subscribeProgress()
    try {
      previewDateActions.value = await invoke('preview_rewrite_date', {
        paths:  _getSortedFilesList(),
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
    if (!scanResult.value?.files?.length) return
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
        paths:  _getSortedFilesList(),
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
    if (!scanResult.value?.files?.length) return
    executing.value   = true
    executingOp.value = 'rename'
    progress.value    = { processed: 0, total: 0 }
    error.value       = null
    lastSummary.value = null

    await _subscribeProgress()
    try {
      lastSummary.value    = await invoke('execute_organize', {
        paths:  _getSortedFilesList(),
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
    if (!scanResult.value?.files?.length) return
    executing.value   = true
    executingOp.value = 'rewrite'
    progress.value    = { processed: 0, total: 0 }
    error.value       = null
    lastSummary.value = null

    await _subscribeProgress()
    try {
      lastSummary.value = await invoke('execute_metadata_rewrite', {
        paths:  _getSortedFilesList(),
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
    stopping.value = true
    await invoke('stop_organize').catch(() => {})
  }

  return {
    folders, config,
    scanning, stopping, scanResult, scanProgress, sortBy, sortDir, activeHistoryEntryId,
    previewing, previewingDate, executing, executingOp, progress,
    previewActions, previewDateActions, previewOnlyRename, lastSummary, error,
    addFolder, removeFolder, updateConfig,
    runScan, runPreviewRewrite, runPreview, runExecute, runMetadataRewrite, stop, loadFromHistory,
  }
})
