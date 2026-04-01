import { ref } from 'vue'
import { check } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'

// ── Shared state ──────────────────────────────────────────────────────────────
const autoCheck        = ref(localStorage.getItem('rustymirror_auto_update') !== 'false')
const notifyOnUpdate   = ref(localStorage.getItem('rustymirror_notify_update') !== 'false')
const status           = ref('idle') // idle | checking | up-to-date | available | downloading | ready | error
const latestVersion    = ref(null)
const downloadProgress = ref(0)
const showNotification = ref(false)

let pendingUpdate = null

// ── Check for updates ─────────────────────────────────────────────────────────
async function checkForUpdates({ notify = false } = {}) {
  if (import.meta.env.DEV) {
    status.value = 'dev'
    return
  }
  status.value = 'checking'
  latestVersion.value = null
  showNotification.value = false
  pendingUpdate = null
  try {
    const update = await check()
    if (update) {
      latestVersion.value = update.version
      status.value = 'available'
      pendingUpdate = update
      if (notify && notifyOnUpdate.value) showNotification.value = true
    } else {
      status.value = 'up-to-date'
    }
  } catch (e) {
    console.error('[updater] check failed:', e)
    status.value = 'error'
  }
}

// ── Download and install ──────────────────────────────────────────────────────
async function installUpdate() {
  if (!pendingUpdate) return
  status.value = 'downloading'
  downloadProgress.value = 0
  try {
    let downloaded = 0
    let total = 0
    await pendingUpdate.downloadAndInstall((event) => {
      if (event.event === 'Started') {
        total = event.data.contentLength ?? 0
      } else if (event.event === 'Progress') {
        downloaded += event.data.chunkLength
        downloadProgress.value = total > 0 ? Math.round((downloaded / total) * 100) : -1
      } else if (event.event === 'Finished') {
        downloadProgress.value = 100
      }
    })
    status.value = 'ready'
  } catch (e) {
    console.error('[updater] install failed:', e)
    status.value = 'error'
  }
}

// ── Restart app ───────────────────────────────────────────────────────────────
async function restartApp() {
  await relaunch()
}

// ── Persist preferences ───────────────────────────────────────────────────────
function saveAutoCheck() {
  localStorage.setItem('rustymirror_auto_update', String(autoCheck.value))
}
function saveNotifyOnUpdate() {
  localStorage.setItem('rustymirror_notify_update', String(notifyOnUpdate.value))
}

export function useUpdater() {
  return {
    autoCheck, notifyOnUpdate, status, latestVersion, downloadProgress, showNotification,
    checkForUpdates, installUpdate, restartApp, saveAutoCheck, saveNotifyOnUpdate,
  }
}
