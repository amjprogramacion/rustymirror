import { ref } from 'vue'
import { openUrl } from '@tauri-apps/plugin-opener'

// ── Repo to check against GitHub Releases API ────────────────────────────────
const GITHUB_REPO = 'amjprogramacion/rustymirror'

// ── Shared state (module-level so persists across component instances) ────────
const autoCheck      = ref(localStorage.getItem('rustymirror_auto_update') !== 'false')
const notifyOnUpdate = ref(localStorage.getItem('rustymirror_notify_update') !== 'false')
const status         = ref('idle') // idle | checking | up-to-date | available | error
const latestVersion  = ref(null)
const showNotification = ref(false)

// ── Semver comparison ─────────────────────────────────────────────────────────
function isNewer(latest, current) {
  const parse = v => v.replace(/^v/, '').split('.').map(Number)
  const [lMaj, lMin, lPat] = parse(latest)
  const [cMaj, cMin, cPat] = parse(current)
  if (lMaj !== cMaj) return lMaj > cMaj
  if (lMin !== cMin) return lMin > cMin
  return lPat > cPat
}

// ── Check for updates ─────────────────────────────────────────────────────────
async function checkForUpdates({ notify = false } = {}) {
  status.value = 'checking'
  latestVersion.value = null
  showNotification.value = false
  try {
    const res = await fetch(`https://api.github.com/repos/${GITHUB_REPO}/releases/latest`, {
      headers: { Accept: 'application/vnd.github+json' }
    })
    if (!res.ok) throw new Error(`GitHub API error: ${res.status}`)
    const data = await res.json()
    const latest  = data.tag_name ?? null
    const current = import.meta.env.VITE_APP_VERSION ?? '0.0.0'
    latestVersion.value = latest
    const available = latest && isNewer(latest, current)
    status.value = available ? 'available' : 'up-to-date'
    if (available && notify && notifyOnUpdate.value) showNotification.value = true
  } catch {
    status.value = 'error'
  }
}

// ── Persist preferences ───────────────────────────────────────────────────────
function saveAutoCheck() {
  localStorage.setItem('rustymirror_auto_update', String(autoCheck.value))
}
function saveNotifyOnUpdate() {
  localStorage.setItem('rustymirror_notify_update', String(notifyOnUpdate.value))
}

// ── Open releases page in browser ────────────────────────────────────────────
async function openReleasePage() {
  try {
    await openUrl(`https://github.com/${GITHUB_REPO}/releases/latest`)
  } catch (e) {
    console.error('[updater] openReleasePage failed:', e)
  }
}

export function useUpdater() {
  return {
    autoCheck, notifyOnUpdate, status, latestVersion, showNotification,
    checkForUpdates, saveAutoCheck, saveNotifyOnUpdate, openReleasePage,
  }
}
