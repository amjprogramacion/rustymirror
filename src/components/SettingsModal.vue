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

          <!-- Body: tabs + content -->
          <div class="modal-body">

            <!-- Left tab rail -->
            <nav class="tab-rail">
              <button
                v-for="tab in tabs"
                :key="tab.id"
                class="tab-item"
                :class="{ 'tab-item--active': activeTab === tab.id }"
                @click="activeTab = tab.id"
              >
                {{ tab.label }}
              </button>
            </nav>

            <!-- Right content -->
            <div class="tab-content">

              <!-- ── General ── -->
              <template v-if="activeTab === 'general'">
                <section class="settings-section">
                  <p class="settings-label">Interface</p>
                  <div class="settings-row">
                    <span class="settings-row-label">Max scan history entries</span>
                    <input
                      type="number"
                      class="settings-input"
                      v-model.number="maxHistory"
                      min="1"
                      max="50"
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
                    />
                  </div>
                </section>

                <div class="settings-divider" />

                <section class="settings-section">
                  <p class="settings-label">Updates</p>
                  <div class="settings-row">
                    <span class="settings-row-label">Check on startup</span>
                    <label class="toggle">
                      <input type="checkbox" v-model="autoCheck" />
                      <span class="toggle-track"><span class="toggle-thumb" /></span>
                    </label>
                  </div>
                  <div class="settings-row">
                    <span class="settings-row-label" :class="{ 'settings-row-label--dim': !autoCheck }">Notify if update found</span>
                    <label class="toggle" :class="{ 'toggle--disabled': !autoCheck }">
                      <input type="checkbox" v-model="notifyOnUpdate" :disabled="!autoCheck" />
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
                  <div v-if="releaseNotes" class="changelog-block" v-html="formattedNotes" />
                </section>

                <div class="settings-divider" />

                <section class="settings-section">
                  <p class="settings-label">About</p>
                  <div class="about-block">
                    <span class="about-name">RustyMirror</span>
                    <span class="about-version">v{{ version }}</span>
                    <p class="about-desc">Duplicate image finder powered by perceptual hashing.</p>
                  </div>
                </section>
              </template>

              <!-- ── Duplicates tool ── -->
              <template v-else-if="activeTab === 'duplicates'">
                <section class="settings-section">
                  <p class="settings-label">Scan options</p>
                  <div class="settings-row">
                    <span class="settings-row-label">Cross-date similarity (phase 5)</span>
                    <label class="toggle">
                      <input type="checkbox" v-model="crossDatePhash" />
                      <span class="toggle-track"><span class="toggle-thumb" /></span>
                    </label>
                  </div>
                  <p class="settings-hint">Re-compare sameDate groups by pHash · does not apply to cached scans</p>
                  <div class="settings-row">
                    <span class="settings-row-label">Fast mode (EXIF thumbnail)</span>
                    <label class="toggle">
                      <input type="checkbox" v-model="fastMode" />
                      <span class="toggle-track"><span class="toggle-thumb" /></span>
                    </label>
                  </div>
                  <p class="settings-hint">Uses the embedded EXIF thumbnail for perceptual hashing (~2× faster on cold scans, slightly less precise)</p>

                  <div class="settings-row">
                    <span class="settings-row-label">Original selection rule</span>
                    <select class="settings-select" :value="dups.retentionRule.kind" @change="onRuleKindChange($event.target.value)">
                      <option value="highestResolution">Highest resolution</option>
                      <option value="oldestDate">Oldest date</option>
                      <option value="newestDate">Newest date</option>
                      <option value="highestSharpness">Sharpest image</option>
                      <option value="filenamePattern">Filename pattern…</option>
                    </select>
                  </div>
                  <p class="settings-hint">
                    Determines which copy within a duplicate group is marked as the original (and skipped by "Select copies").
                    <template v-if="dups.retentionRule.kind === 'highestResolution'">Keeps the file with the most pixels, preferring files without "copy/copia" in the name.</template>
                    <template v-else-if="dups.retentionRule.kind === 'oldestDate'">Keeps the earliest capture date (EXIF DateTimeOriginal, or file mtime as fallback).</template>
                    <template v-else-if="dups.retentionRule.kind === 'newestDate'">Keeps the most recent capture date.</template>
                    <template v-else-if="dups.retentionRule.kind === 'highestSharpness'">Keeps the sharpest copy based on Laplacian variance · requires a fresh scan to compute scores.</template>
                    <template v-else-if="dups.retentionRule.kind === 'filenamePattern'">Keeps the file whose name matches the glob pattern below · falls back to highest resolution if no match.</template>
                  </p>
                  <template v-if="dups.retentionRule.kind === 'filenamePattern'">
                    <div class="settings-row">
                      <span class="settings-row-label">Pattern</span>
                      <input
                        class="settings-input settings-input--wide"
                        type="text"
                        placeholder="e.g. IMG_* or DSC_*"
                        :value="dups.retentionRule.pattern ?? ''"
                        @change="onPatternChange($event.target.value)"
                      />
                    </div>
                    <p class="settings-hint">Case-insensitive glob matched against the filename stem (without extension). Supports * and ?.</p>
                  </template>
                </section>

                <div class="settings-divider" />

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
              </template>

              <!-- ── Organizer tool ── -->
              <template v-else-if="activeTab === 'organizer'">
                <section class="settings-section">
                  <p class="settings-label">Naming</p>
                  <div class="settings-row">
                    <span class="settings-row-label">Fallback year</span>
                    <input
                      type="number"
                      class="settings-input"
                      :value="org.config.yearIfNotDate"
                      min="1900"
                      max="2100"
                      @change="org.updateConfig({ yearIfNotDate: +$event.target.value })"
                    />
                  </div>
                  <p class="settings-hint">Year used when no date can be extracted from EXIF or filename.</p>
                </section>
              </template>

              <!-- ── Metadata tool ── -->
              <template v-else-if="activeTab === 'metadata'">
                <section class="settings-section">
                  <p class="settings-label">Scan options</p>
                  <div class="settings-row">
                    <span class="settings-row-label">Prefetch filters</span>
                    <label class="toggle">
                      <input type="checkbox" v-model="prefetchFilters" />
                      <span class="toggle-track"><span class="toggle-thumb" /></span>
                    </label>
                  </div>
                  <p class="settings-hint">Waits for all location lookups to finish before showing results — slower scan but filter dropdowns are fully populated from the start. When off, options appear progressively as queries complete.</p>
                </section>

                <div class="settings-divider" />

                <section class="settings-section">
                  <p class="settings-label">Data</p>
                  <div class="settings-row">
                    <span class="settings-row-label">
                      Geo cache
                      <span class="cache-hint" v-if="meta.geoCacheCount > 0">
                        {{ meta.geoCacheCount }} entries · {{ formatSize(meta.geoCacheBytes) }}
                      </span>
                    </span>
                    <button
                      class="btn-setting"
                      :class="{ 'btn-setting--active': meta.geoCacheCount > 0 }"
                      :disabled="meta.geoCacheCount === 0"
                      @click="meta.clearGeoCache()"
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
              </template>

            </div>
          </div>

        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { useDuplicatesHistoryStore } from '../store/duplicatesHistory'
import { useDuplicatesStore } from '../store/duplicates'
import { useMetadataStore } from '../store/metadata'
import { useOrganizerStore } from '../store/organizer'
import { useCacheSize } from '../composables/useCacheSize'
import { useUpdater } from '../composables/useUpdater'
import { useSettings } from '../composables/useSettings'
import { formatSize } from '../utils/formatters'

defineProps({ modelValue: Boolean })
defineEmits(['update:modelValue'])

const tabs = [
  { id: 'general',    label: 'General' },
  { id: 'duplicates', label: 'Duplicates tool' },
  { id: 'metadata',   label: 'Metadata tool' },
  { id: 'organizer',  label: 'Organizer tool' },
]
const activeTab = ref('general')

const history = useDuplicatesHistoryStore()
const dups    = useDuplicatesStore()
const meta    = useMetadataStore()
const org     = useOrganizerStore()

function onRuleKindChange(kind) {
  const rule = kind === 'filenamePattern'
    ? { kind, pattern: dups.retentionRule.pattern ?? '' }
    : { kind }
  dups.applyRetentionRule(rule)
}

function onPatternChange(pattern) {
  dups.applyRetentionRule({ kind: 'filenamePattern', pattern })
}
const version = ref(import.meta.env.VITE_APP_VERSION ?? '0.1.0')

const { maxHistory, thumbConcurrency, crossDatePhash, fastMode, autoUpdate: autoCheck, notifyOnUpdate, prefetchFilters } = useSettings()

const { cacheSize, thumbCacheSize, loadCacheSizes, clearCache, clearThumbCache } = useCacheSize()
const { status: updateStatus, latestVersion, releaseNotes, downloadProgress, errorMessage, checkForUpdates, installUpdate, restartApp } = useUpdater()

const formattedNotes = computed(() => {
  if (!releaseNotes.value) return ''
  return releaseNotes.value
    .split('\n')
    .map(line => {
      if (line.startsWith('### ')) return `<span class="notes-group">${line.slice(4)}</span>`
      if (line.startsWith('- '))   return `<span class="notes-item">• ${line.slice(2)}</span>`
      return null
    })
    .filter(Boolean)
    .join('')
})

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
  width: 640px;
  max-width: calc(100vw - 32px);
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

/* ── Header ── */
.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-4) var(--space-4) var(--space-3);
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
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

/* ── Body layout ── */
.modal-body {
  display: flex;
  min-height: 320px;
}

/* ── Tab rail (left) ── */
.tab-rail {
  width: 148px;
  flex-shrink: 0;
  border-right: 1px solid var(--border-color);
  padding: var(--space-3) var(--space-2);
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.tab-item {
  text-align: left;
  padding: 7px var(--space-3);
  border-radius: var(--border-radius-sm);
  font-size: var(--font-size-sm);
  color: var(--text-secondary);
  background: transparent;
  border: none;
  cursor: pointer;
  transition: background var(--transition), color var(--transition);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.tab-item:hover {
  background: var(--bg-card);
  color: var(--text-primary);
}
.tab-item--active {
  background: var(--bg-card);
  color: var(--text-primary);
  font-weight: 500;
}

/* ── Tab content (right) ── */
.tab-content {
  flex: 1;
  overflow-y: auto;
  min-width: 0;
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
.settings-input--wide { width: 140px; }

/* ── Select ── */
.settings-select {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-primary);
  font-size: var(--font-size-sm);
  padding: 4px var(--space-2);
  cursor: pointer;
}
.settings-select:focus {
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

/* ── Changelog block ── */
.changelog-block {
  display: flex;
  flex-direction: column;
  gap: 3px;
  max-height: 160px;
  overflow-y: auto;
  padding: 8px 10px;
  background: var(--bg-card);
  border-radius: var(--border-radius-sm);
  font-size: 11px;
  line-height: 1.5;
  margin-top: -4px;
}

.changelog-block :deep(.notes-group) {
  display: block;
  font-weight: 600;
  color: var(--text-secondary);
  margin-top: 6px;
  text-transform: uppercase;
  font-size: 10px;
  letter-spacing: 0.04em;
}

.changelog-block :deep(.notes-group:first-child) {
  margin-top: 0;
}

.changelog-block :deep(.notes-item) {
  display: block;
  color: var(--text-muted);
  padding-left: 4px;
}

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
