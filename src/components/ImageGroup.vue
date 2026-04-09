<template>
  <div class="group" ref="groupEl">
    <div class="group-header">
      <span class="kind-badge" :class="group.kind">
        {{ kindLabel(group.kind) }}
        <span v-if="group.similarity != null && group.kind !== 'exact'"
              class="similarity-pct">{{ group.similarity }}%</span>
      </span>
      <span class="group-count">{{ group.entries.length }} images</span>
    </div>

    <div class="cards">
      <div
        v-for="(entry, idx) in group.entries"
        :key="entry.path"
        class="card"
        :class="{ selected: store.selected.has(entry.path), original: entry.isOriginal, focused: focusedPath === entry.path }"
        :tabindex="0"
        :data-card-path="entry.path"
        @click="onCardClick(entry)"
        @keydown="onCardKeydown($event, entry, idx)"
        @focus="focusedPath = entry.path"
        @blur="focusedPath = null"
      >
        <div class="thumb-wrap" :data-path="entry.path">
          <img
            v-if="store.directSrcCache[entry.path]"
            :src="store.directSrcCache[entry.path]"
            class="thumb"
            draggable="false"
            @error="onDirectSrcError(entry.path)"
          />
          <img
            v-else-if="store.thumbCache[entry.path] && store.thumbCache[entry.path] !== THUMB_ERROR"
            :src="store.thumbCache[entry.path]"
            class="thumb"
            draggable="false"
          />
          <div v-else-if="store.thumbCache[entry.path] === THUMB_ERROR" class="thumb-placeholder">
            <span class="thumb-ext">{{ fileExt(entry.path).toUpperCase() }}</span>
            <span class="thumb-no-preview">No preview</span>
          </div>
          <div v-else class="thumb-placeholder">
            <span class="thumb-loading" />
          </div>
          <span class="original-badge" v-if="entry.isOriginal">Original</span>
          <div class="selected-overlay" v-if="store.selected.has(entry.path)">
            <span class="checkmark">&#10003;</span>
          </div>
        </div>

        <div class="meta">
          <p class="meta-name" :title="entry.path">{{ fileName(entry.path) }}</p>
          <p class="meta-detail">
            {{ entry.width > 0 ? `${entry.width}x${entry.height}` : '--' }} · {{ formatSize(entry.sizeBytes) }}
          </p>
          <p class="meta-detail">{{ formatDate(entry.dateTaken ?? entry.modified) }}</p>
          <div class="meta-actions">
            <div class="btn-group">
              <button class="btn-open btn-explore" @click.stop="openFolder(entry.path)" title="Show in folder">Explore</button>
              <button class="btn-open" @click.stop="openFile(entry.path)" title="Open file">Open</button>
              <button class="btn-open btn-info" @click.stop="store.openMetadataPanel(entry)" title="View metadata">EXIF</button>
            </div>
            <label v-if="!store.multiSelect" class="card-checkbox" @click.stop>
              <input
                type="checkbox"
                :checked="store.selected.has(entry.path)"
                @change="store.toggleSelected(entry.path)"
              />
            </label>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted, onBeforeUnmount, nextTick } from 'vue'
import { convertFileSrc, invoke } from '@tauri-apps/api/core'
import { useScanStore } from '../store/scan'
import { fileExt, fileName, formatSize, formatDate, kindLabel } from '../utils/formatters'

const props = defineProps({ group: { type: Object, required: true } })
const store = useScanStore()
const THUMB_ERROR = '__error__'
const HEIC_EXTS = new Set(['heic', 'heif'])
// PNGs are always processed via Rust to avoid WebView2 rendering issues with
// certain PNG variants (16-bit depth, Display P3 / ICC color profiles, etc.).
// Rust decodes and re-encodes as a plain JPEG thumbnail, which WebView2 handles fine.
const RUST_THUMB_EXTS = new Set(['png'])

let observer = null
const groupEl = ref(null)

function needsRust(path) {
  const ext = fileExt(path)
  return HEIC_EXTS.has(ext) || RUST_THUMB_EXTS.has(ext) || store.isNetworkPath(path)
}

onMounted(() => {
  for (const entry of props.group.entries) {
    if (!needsRust(entry.path) && !(entry.path in store.directSrcCache)) {
      store.setDirectSrc(entry.path, convertFileSrc(entry.path))
    }
  }

  const toObserve = props.group.entries.filter(e =>
    needsRust(e.path) && !(e.path in store.thumbCache)
  )

  if (toObserve.length === 0) return

  observer = new IntersectionObserver((entries) => {
    for (const e of entries) {
      const path = e.target.dataset.cardPath
      if (!path) continue
      if (path in store.thumbCache) {
        observer.unobserve(e.target)
        continue
      }
      if (e.isIntersecting) {
        store.enqueueThumbnail(path)
      } else {
        store.dequeueThumbnail(path)
      }
    }
  }, { rootMargin: '400px', threshold: 0 })

  nextTick(() => {
    if (!groupEl.value) return
    const pathsToObserve = new Set(toObserve.map(e => e.path))
    const cards = groupEl.value.querySelectorAll('.card[data-card-path]')
    cards.forEach(el => {
      const path = el.dataset.cardPath
      if (pathsToObserve.has(path) && !(path in store.thumbCache)) {
        observer.observe(el)
      }
    })
  })
})

onBeforeUnmount(() => observer?.disconnect())

// Called when the browser fails to load a file via convertFileSrc (e.g. very
// large PNG, unusual color profile, WebView2 rendering issue). Clear the direct
// src so the template falls through to the thumb path, then ask Rust to decode
// and serve the file — Rust now falls back to raw bytes if it can't resize.
function onDirectSrcError(path) {
  store.directSrcCache[path] = null
  store.enqueueThumbnail(path)
}

const focusedPath = ref(null)

function onCardClick(entry) {
  if (store.multiSelect) {
    store.toggleSelected(entry.path)
  } else {
    const idx = props.group.entries.findIndex(e => e.path === entry.path)
    store.openLightbox(props.group.entries, idx >= 0 ? idx : 0)
  }
}

function onCardKeydown(e, entry, idx) {
  if (store.lightbox) return
  if (e.key === ' ') {
    e.preventDefault()
    store.toggleSelected(entry.path)
    return
  }
  if (e.key === 'Enter') {
    e.preventDefault()
    store.openLightbox(props.group.entries, idx)
    return
  }
  if (e.key === 'ArrowRight') {
    e.preventDefault()
    focusCardInGroup(idx + 1)
    return
  }
  if (e.key === 'ArrowLeft') {
    e.preventDefault()
    focusCardInGroup(idx - 1)
    return
  }
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault()
    focusAdjacentGroup(e.key === 'ArrowDown' ? 1 : -1, idx)
  }
}

function focusCardInGroup(targetIdx) {
  if (targetIdx < 0 || targetIdx >= props.group.entries.length) return
  const path = props.group.entries[targetIdx].path
  document.querySelector(`[data-card-path="${CSS.escape(path)}"]`)?.focus()
}

function focusAdjacentGroup(direction, currentIdx) {
  const allGroups = [...document.querySelectorAll('.group')]
  const currentCard = document.querySelector(`[data-card-path="${CSS.escape(props.group.entries[currentIdx].path)}"]`)
  const currentGroup = currentCard?.closest('.group')
  if (!currentGroup) return
  const groupIdx = allGroups.indexOf(currentGroup)
  const targetGroup = allGroups[groupIdx + direction]
  if (!targetGroup) return
  const targetCards = [...targetGroup.querySelectorAll('[data-card-path]')]
  const targetCard = targetCards[Math.min(currentIdx, targetCards.length - 1)]
  targetCard?.focus()
  targetCard?.scrollIntoView({ block: 'nearest' })
}

async function openFile(path) {
  await invoke('open_file', { path })
}

async function openFolder(path) {
  await invoke('open_folder', { path })
}

</script>

<style scoped>
.group { display: flex; flex-direction: column; gap: var(--space-3); }

.group-header { display: flex; align-items: center; gap: var(--space-2); }

.kind-badge {
  font-size: var(--font-size-xs);
  font-weight: 600;
  padding: 4px 8px;
  border-radius: var(--border-radius-pill);
  text-transform: uppercase;
  letter-spacing: 0.4px;
}
.kind-badge.exact    { background: #2a3f6e; color: #7aabff; }
.kind-badge.similar  { background: #3a2e1a; color: #f0a840; }
.kind-badge.sameDate { background: #1e3a2a; color: #5fcf90; }

.similarity-pct {
  opacity: 0.8;
  margin-left: 4px;
  font-weight: 400;
}
.group-count { font-size: var(--font-size-xs); color: var(--text-muted); }

.cards {
  display: grid;
  grid-template-columns: repeat(6, 1fr);
  gap: var(--space-3);
}

.card {
  border-radius: var(--border-radius-md);
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  overflow: hidden;
  cursor: default;
  transition: border-color var(--transition), background var(--transition);
  contain: layout style;
  position: relative;
}

/* ::after overlay covers the entire card surface including the image area.
   It renders a crisp inner ring + soft glow without being affected by
   thumb-wrap's overflow:hidden, and never escapes the card boundary. */
.card::after {
  content: '';
  position: absolute;
  inset: 0;
  border-radius: var(--border-radius-md);
  pointer-events: none;
  border: 2px solid transparent;
  box-shadow: none;
  transition: border-color var(--transition), box-shadow var(--transition);
  z-index: 10;
}
.card.focused::after {
  border-color: #7ab8f5;
  box-shadow: inset 0 0 18px 4px rgba(122, 184, 245, 0.3);
}

.meta-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 4px;
}

.btn-group {
  display: flex;
  gap: 3px;
}

.card-checkbox {
  display: flex;
  align-items: center;
  cursor: pointer;
}

.card-checkbox input[type="checkbox"] {
  width: 14px;
  height: 14px;
  accent-color: var(--color-accent);
  cursor: pointer;
  opacity: 0.5;
  transition: opacity var(--transition);
}

.card:hover .card-checkbox input[type="checkbox"],
.card-checkbox input[type="checkbox"]:checked {
  opacity: 1;
}
.card:hover    { background: var(--bg-card-hover); }
.card:focus    { outline: none; }
.card.selected { border-color: var(--color-accent); background: var(--bg-card-selected); }
.card.original { border-color: var(--color-success); }

.thumb-wrap {
  position: relative;
  width: 100%;
  height: 150px;
  background: #111;
  overflow: hidden;
}
.thumb { width: 100%; height: 100%; object-fit: cover; display: block; }

.thumb-placeholder {
  width: 100%; height: 100%;
  display: flex; flex-direction: column;
  align-items: center; justify-content: center;
  gap: 4px; background: #1a1a1a;
}
.thumb-ext {
  font-size: 11px; font-weight: 700; color: var(--text-muted);
  background: var(--bg-card); padding: 2px 6px;
  border-radius: var(--border-radius-sm);
  border: 1px solid var(--border-color);
}
.thumb-no-preview { font-size: 9px; color: var(--text-muted); opacity: 0.6; }

@keyframes spin { to { transform: rotate(360deg); } }
.thumb-loading {
  width: 20px; height: 20px;
  border: 2px solid #333;
  border-top-color: var(--color-accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

.original-badge {
  position: absolute; top: 4px; left: 4px;
  background: var(--color-success); color: #fff;
  font-size: 10px; font-weight: 600;
  padding: 2px 6px; border-radius: var(--border-radius-pill);
}
.selected-overlay {
  position: absolute; inset: 0;
  background: rgba(74,144,217,0.35);
  display: flex; align-items: center; justify-content: center;
}
.checkmark { font-size: 28px; color: #fff; text-shadow: 0 1px 4px rgba(0,0,0,0.5); }

.meta {
  padding: var(--space-2);
  display: flex; flex-direction: column; gap: 2px;
}
.meta-name {
  font-size: var(--font-size-xs); color: var(--text-primary);
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  font-weight: 500;
}
.meta-detail { font-size: 10px; color: var(--text-muted); }

.btn-open {
  margin-top: 0;
  padding: 2px 6px;
  font-size: 10px; font-weight: 500;
  background: var(--bg-secondary);
  color: var(--text-secondary);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  cursor: pointer; align-self: flex-start;
  transition: background var(--transition), color var(--transition), border-color var(--transition);
}
.btn-open:hover       { background: var(--color-accent);  color: #fff; border-color: var(--color-accent);  }
.btn-explore:hover    { background: var(--color-success); color: #fff; border-color: var(--color-success); }
.btn-info:hover       { background: #7c3aed; color: #fff; border-color: #7c3aed; }
</style>
