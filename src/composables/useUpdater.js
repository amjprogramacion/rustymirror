import { ref } from 'vue'
import { check } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import { useSettings } from './useSettings'

const GITHUB_RELEASES_API = 'https://api.github.com/repos/amjprogramacion/rustymirror/releases?per_page=100'

// ── Shared state ──────────────────────────────────────────────────────────────
const { autoUpdate: autoCheck, notifyOnUpdate } = useSettings()
const status           = ref('idle') // idle | checking | up-to-date | available | downloading | ready | error
const latestVersion    = ref(null)
const releaseNotes     = ref(null)
const versionNotes     = ref([])     // [{ version, notes }] newest-first, every release between installed and latest
const downloadProgress = ref(0)
const showNotification = ref(false)
const errorMessage     = ref('')

let pendingUpdate = null

// Compares two dotted numeric versions ("1.5.2.3"), ignoring a leading "v".
// Missing trailing components count as 0. Returns -1, 0 or 1.
function compareVersions(a, b) {
  const pa = String(a).replace(/^v/, '').split('.').map(Number)
  const pb = String(b).replace(/^v/, '').split('.').map(Number)
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const x = pa[i] ?? 0
    const y = pb[i] ?? 0
    if (x !== y) return x < y ? -1 : 1
  }
  return 0
}

// Fetches release notes for every published release strictly newer than the
// installed version up to and including the latest, newest-first. Falls back to
// the single latest body if GitHub is unreachable or returns nothing usable.
async function fetchIntermediateNotes(current, latest, latestBody) {
  const fallback = [{ version: latest, notes: latestBody ?? '' }]
  // Without a known installed version we can't bound the range — show only latest.
  if (!current) return fallback
  try {
    const res = await fetch(GITHUB_RELEASES_API, { headers: { Accept: 'application/vnd.github+json' } })
    if (!res.ok) throw new Error(`GitHub API ${res.status}`)
    const releases = await res.json()
    const notes = releases
      .filter(r => !r.draft && !r.prerelease)
      .map(r => ({ version: String(r.tag_name).replace(/^v/, ''), notes: (r.body ?? '').trim() }))
      .filter(r => compareVersions(r.version, current) > 0 && compareVersions(r.version, latest) <= 0)
      .sort((a, b) => compareVersions(b.version, a.version))
    return notes.length ? notes : fallback
  } catch (e) {
    console.warn('[updater] changelog fetch failed:', e)
    return fallback
  }
}

// ── Check for updates ─────────────────────────────────────────────────────────
async function checkForUpdates({ notify = false, silent = false } = {}) {
  status.value = 'checking'
  latestVersion.value = null
  releaseNotes.value = null
  versionNotes.value = []
  showNotification.value = false
  pendingUpdate = null
  try {
    const update = await check()
    if (update) {
      latestVersion.value = update.version
      releaseNotes.value = update.body ?? null
      status.value = 'available'
      pendingUpdate = update
      // Pull notes for all intermediate releases, not just the latest.
      versionNotes.value = await fetchIntermediateNotes(update.currentVersion, update.version, update.body)
      if (notify && notifyOnUpdate.value) showNotification.value = true
    } else {
      status.value = 'up-to-date'
    }
  } catch (e) {
    console.error('[updater] check failed:', e)
    errorMessage.value = String(e)
    status.value = silent ? 'idle' : 'error'
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
    errorMessage.value = String(e)
    status.value = 'error'
  }
}

// ── Restart app ───────────────────────────────────────────────────────────────
async function restartApp() {
  await relaunch()
}

export function useUpdater() {
  return {
    autoCheck, notifyOnUpdate, status, latestVersion, releaseNotes, versionNotes, downloadProgress, showNotification, errorMessage,
    checkForUpdates, installUpdate, restartApp,
  }
}
