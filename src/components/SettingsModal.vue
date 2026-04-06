<template>
  <Teleport to="body">
    <Transition name="modal-fade">
      <div v-if="modelValue" class="modal-overlay" @click.self="$emit('update:modelValue', false)">
        <div class="modal-box" role="dialog" aria-modal="true" aria-label="Settings">

          <!-- Header -->
          <div class="modal-header">
            <span class="modal-title">Settings</span>
            <button class="modal-close" @click="$emit('update:modelValue', false)" title="Close">✕</button>
          </div>

          <!-- General -->
          <section class="settings-section">
            <p class="settings-label">General</p>
            <div class="settings-row">
              <span class="settings-row-label">Max scan history entries</span>
              <input
                type="number"
                class="settings-input"
                v-model.number="maxHistory"
                min="1"
                max="50"
                @change="saveMaxHistory"
              />
            </div>
            <div class="settings-row">
              <span class="settings-row-label">Concurrent thumbnail loads</span>
              <input
                type="number"
                class="settings-input"
                v-model.number="thumbConcurrency"
                min="1"
                max="16"
                @change="saveThumbConcurrency"
              />
            </div>
            <div class="settings-row">
              <span class="settings-row-label">Cross-date similarity (phase 5)</span>
              <label class="toggle">
                <input type="checkbox" v-model="crossDatePhash" @change="saveCrossDatePhash" />
                <span class="toggle-track"><span class="toggle-thumb" /></span>
              </label>
            </div>
            <p class="settings-hint">Re-compare sameDate groups by pHash · does not apply to cached scans</p>
            <div class="settings-row">
              <span class="settings-row-label">Fast mode (EXIF thumbnail)</span>
              <label class="toggle">
                <input type="checkbox" v-model="fastMode" @change="saveFastMode" />
                <span class="toggle-track"><span class="toggle-thumb" /></span>
              </label>
            </div>
            <p class="settings-hint">Uses the embedded EXIF thumbnail for perceptual hashing (~2× faster on cold scans, slightly less precise)</p>
          </section>

          <div class="settings-divider" />

          <!-- Data -->
          <section class="settings-section">
            <p class="settings-label">Data</p>
            <div class="settings-row">
              <span class="settings-row-label">Scan history</span>
              <button
                class="btn-setting"
                :class="{ 'btn-setting--active': history.entries.length > 0 }"
                :disabled="history.entries.length === 0"
                @click="history.clearHistory()"
              >
                Clear ({{ history.entries.length }})
              </button>
            </div>
            <div class="settings-row">
              <span class="settings-row-label">
                Hash cache
                <span class="cache-hint" v-if="cacheSize > 0">{{ formatSize(cacheSize) }}</span>
              </span>
              <button
                class="btn-setting"
                :class="{ 'btn-setting--active': cacheSize > 0 }"
                :disabled="cacheSize === 0"
                @click="clearCache"
              >
                Clear
              </button>
            </div>
            <div class="settings-row">
              <span class="settings-row-label">
                Thumbnail cache
                <span class="cache-hint" v-if="thumbCacheSize > 0">{{ formatSize(thumbCacheSize) }}</span>
              </span>
              <button
                class="btn-setting"
                :class="{ 'btn-setting--active': thumbCacheSize > 0 }"
                :disabled="thumbCacheSize === 0"
                @click="clearThumbCache"
              >
                Clear
              </button>
            </div>
          </section>

          <div class="settings-divider" />

          <!-- Updates -->
          <section class="settings-section">
            <p class="settings-label">Updates</p>
            <div class="settings-row">
              <span class="settings-row-label">Check on startup</span>
              <label class="toggle">
                <input type="checkbox" v-model="autoCheck" @change="saveAutoCheck" />
                <span class="toggle-track"><span class="toggle-thumb" /></span>
              </label>
            </div>
            <div class="settings-row">
              <span class="settings-row-label" :class="{ 'settings-row-label--dim': !autoCheck }">Notify if update found</span>
              <label class="toggle" :class="{ 'toggle--disabled': !autoCheck }">
                <input type="checkbox" v-model="notifyOnUpdate" @change="saveNotifyOnUpdate" :disabled="!autoCheck" />
                <span class="toggle-track"><span class="toggle-thumb" /></span>
              </label>
            </div>
            <div class="settings-row">
              <span class="settings-row-label update-status">
                <span v-if="updateStatus === 'idle'">Not checked yet</span>
                <span v-else-if="updateStatus === 'dev'" class="status-checking">Not available in dev mode</span>
                <span v-else-if="updateStatus === 'checking'" class="status-checking">Checking…</span>
                <span v-else-if="updateStatus === 'up-to-date'" class="status-ok">Up to date</span>
                <span v-else-if="updateStatus === 'available'" class="status-available">Update available: {{ latestVersion }}</span>
                <span v-else-if="updateStatus === 'downloading'" class="status-checking">
                  Downloading{{ downloadProgress >= 0 ? ` ${downloadProgress}%` : '…' }}
                </span>
                <span v-else-if="updateStatus === 'ready'" class="status-ok">Installed — restart to apply</span>
                <span v-else-if="updateStatus === 'error'" class="status-error">Error</span>
              </span>
              <button
                class="btn-setting btn-setting--active"
                :disabled="updateStatus === 'checking'"
                @click="checkForUpdates()"
              >
                {{ updateStatus === 'checking' ? 'Checking…' : 'Check now' }}
              </button>
            </div>
            <div v-if="updateStatus === 'error' && errorMessage" class="update-error-detail">
              {{ errorMessage }}
            </div>
            <div v-if="updateStatus === 'available'" class="settings-row">
              <button class="btn-setting btn-setting--update btn-setting--full" @click="installUpdate">
                Install {{ latestVersion }}
              </button>
            </div>
            <div v-if="updateStatus === 'downloading'" class="update-progress-wrap">
              <div class="update-progress-bar" :style="{ width: downloadProgress >= 0 ? `${downloadProgress}%` : '100%' }" />
            </div>
            <div v-if="updateStatus === 'ready'" class="settings-row">
              <button class="btn-setting btn-setting--update btn-setting--full" @click="restartApp">
                Restart now
              </button>
            </div>
          </section>

          <div class="settings-divider" />

          <!-- About -->
          <section class="settings-section">
            <p class="settings-label">About</p>
            <div class="about-block">
              <span class="about-name">RustyMirror</span>
              <span class="about-version">v{{ version }}</span>
              <p class="about-desc">Duplicate image finder powered by perceptual hashing.</p>
            </div>
          </section>

        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { useHistoryStore } from '../store/history'
import { useCacheSize } from '../composables/useCacheSize'
import { useUpdater } from '../composables/useUpdater'
import { formatSize } from '../utils/formatters'

defineProps({ modelValue: Boolean })
defineEmits(['update:modelValue'])

const history = useHistoryStore()
const version = ref(import.meta.env.VITE_APP_VERSION ?? '0.1.0')

const maxHistory = ref(parseInt(localStorage.getItem('rustymirror_max_history') ?? '5', 10))
function saveMaxHistory() {
  localStorage.setItem('rustymirror_max_history', String(maxHistory.value))
}

const thumbConcurrency = ref(parseInt(localStorage.getItem('rustymirror_thumb_concurrency') ?? '4', 10))
function saveThumbConcurrency() {
  localStorage.setItem('rustymirror_thumb_concurrency', String(thumbConcurrency.value))
}

const crossDatePhash = ref(localStorage.getItem('rustymirror_cross_date_phash') !== 'false')
function saveCrossDatePhash() {
  localStorage.setItem('rustymirror_cross_date_phash', String(crossDatePhash.value))
}

const fastMode = ref(localStorage.getItem('rustymirror_fast_mode') === 'true')
function saveFastMode() {
  localStorage.setItem('rustymirror_fast_mode', String(fastMode.value))
}

const { cacheSize, thumbCacheSize, loadCacheSizes, clearCache, clearThumbCache } = useCacheSize()
const { autoCheck, notifyOnUpdate, status: updateStatus, latestVersion, downloadProgress, errorMessage, checkForUpdates, installUpdate, restartApp, saveAutoCheck, saveNotifyOnUpdate } = useUpdater()

onMounted(loadCacheSizes)
</script>

<style scoped>
/* ── Overlay ── */
.modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 300;
}

/* ── Box ── */
.modal-box {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-lg);
  width: 600px;
  max-width: calc(100vw - 32px);
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
  overflow: hidden;
}

/* ── Header ── */
.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-4) var(--space-4) var(--space-3);
  border-bottom: 1px solid var(--border-color);
}

.modal-title {
  font-size: var(--font-size-md);
  font-weight: 600;
  color: var(--text-primary);
}

.modal-close {
  background: none;
  color: var(--text-muted);
  font-size: 12px;
  padding: 4px;
  border-radius: var(--border-radius-sm);
  transition: color var(--transition), background var(--transition);
}
.modal-close:hover {
  color: var(--text-primary);
  background: var(--bg-card);
}

/* ── Sections ── */
.settings-section {
  padding: var(--space-4);
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.settings-label {
  font-size: var(--font-size-xs);
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.6px;
  margin-bottom: 2px;
}

.settings-divider {
  height: 1px;
  background: var(--border-color);
}

/* ── Row ── */
.settings-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
}

.settings-row-label {
  font-size: var(--font-size-sm);
  color: var(--text-secondary);
  display: flex;
  align-items: center;
  gap: var(--space-1);
}

.cache-hint {
  font-size: 10px;
  color: var(--text-muted);
}

.settings-hint {
  font-size: 10px;
  color: var(--text-muted);
  line-height: 1.4;
  margin-top: -8px;
}

/* ── Input ── */
.settings-input {
  width: 64px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-primary);
  font-size: var(--font-size-sm);
  padding: 4px var(--space-2);
  text-align: left;
}
.settings-input:focus {
  outline: none;
  border-color: var(--color-accent);
}

/* ── Setting button ── */
.btn-setting {
  font-size: 11px;
  font-weight: 500;
  padding: 4px 10px;
  border-radius: var(--border-radius-sm);
  background: transparent;
  color: var(--text-muted);
  border: 1px solid transparent;
  transition: background var(--transition), color var(--transition), border-color var(--transition);
}
.btn-setting:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}
.btn-setting.btn-setting--active {
  background: var(--bg-card);
  color: var(--text-secondary);
  border-color: var(--border-color);
}
.btn-setting.btn-setting--active:hover {
  background: var(--color-danger);
  color: #fff;
  border-color: var(--color-danger);
}

/* ── About ── */
.about-block {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.about-name {
  font-size: var(--font-size-md);
  font-weight: 600;
  color: var(--text-primary);
}

.about-version {
  font-size: var(--font-size-xs);
  color: var(--color-accent);
}

.about-desc {
  font-size: var(--font-size-xs);
  color: var(--text-muted);
  margin-top: 2px;
  line-height: 1.5;
}

/* ── Toggle ── */
.toggle {
  position: relative;
  display: inline-flex;
  align-items: center;
  cursor: pointer;
  flex-shrink: 0;
}
.toggle input {
  position: absolute;
  opacity: 0;
  width: 0;
  height: 0;
}
.toggle-track {
  width: 32px;
  height: 18px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-pill);
  transition: background var(--transition), border-color var(--transition);
  display: flex;
  align-items: center;
  padding: 2px;
}
.toggle input:checked + .toggle-track {
  background: var(--color-accent);
  border-color: var(--color-accent);
}
.toggle-thumb {
  width: 12px;
  height: 12px;
  background: var(--text-muted);
  border-radius: 50%;
  transition: transform var(--transition), background var(--transition);
}
.toggle input:checked + .toggle-track .toggle-thumb {
  transform: translateX(14px);
  background: #fff;
}

/* ── Dim label when dependency toggle is off ── */
.settings-row-label--dim { opacity: 0.35; }
.toggle--disabled { opacity: 0.35; cursor: not-allowed; }

/* ── Update progress bar ── */
.update-progress-wrap {
  height: 4px;
  background: var(--bg-card);
  border-radius: 2px;
  overflow: hidden;
  margin-top: -4px;
}
.update-progress-bar {
  height: 100%;
  background: #f5c542;
  border-radius: 2px;
  transition: width 0.3s ease;
}

/* ── Update status ── */
.update-status { font-size: var(--font-size-sm); }
.status-checking { color: var(--text-muted); font-style: italic; }
.status-ok       { color: var(--color-success); }
.status-available{ color: #f5c542; font-weight: 600; }
.status-error    { color: var(--color-danger); }

.update-error-detail {
  font-size: 11px;
  color: var(--color-danger);
  background: rgba(220, 53, 69, 0.08);
  border: 1px solid rgba(220, 53, 69, 0.25);
  border-radius: var(--border-radius-sm);
  padding: 6px 10px;
  word-break: break-all;
  line-height: 1.5;
}

.btn-setting--update {
  background: #f5c542;
  color: #1a1200;
  border-color: #f5c542;
  font-weight: 600;
}
.btn-setting--update:hover {
  background: #f0b800;
  border-color: #f0b800;
}
.btn-setting--full { width: 100%; justify-content: center; }

/* ── Transition ── */
.modal-fade-enter-active,
.modal-fade-leave-active {
  transition: opacity 150ms ease;
}
.modal-fade-enter-active .modal-box,
.modal-fade-leave-active .modal-box {
  transition: transform 150ms ease, opacity 150ms ease;
}
.modal-fade-enter-from,
.modal-fade-leave-to {
  opacity: 0;
}
.modal-fade-enter-from .modal-box,
.modal-fade-leave-to .modal-box {
  transform: scale(0.95);
  opacity: 0;
}
</style>
