<template>
  <Transition name="lb-fade">
    <div v-if="store.lightbox" class="lb-overlay" @click="onOverlayClick">

      <!-- Close -->
      <button class="lb-close" @click="store.closeLightbox()">✕</button>

      <!-- Prev -->
      <button
        v-if="entries.length > 1"
        class="lb-nav lb-prev"
        @click="store.lightboxPrev()"
      >
        <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
          <path d="M13 4L7 10L13 16" stroke="white" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      </button>

      <!-- Image -->
      <div class="lb-frame">
        <img
          v-if="directSrc"
          ref="imgEl"
          :src="directSrc"
          class="lb-img"
          draggable="false"
          @load="updateBadgePos"
        />
        <div v-else class="lb-spinner" />
        <!-- Badge positioned via JS to sit over the actual image area -->
        <div
          class="lb-badge"
          v-if="current.isOriginal && directSrc && badgeStyle"
          :style="badgeStyle"
        >Original</div>
      </div>

      <!-- Next -->
      <button
        v-if="entries.length > 1"
        class="lb-nav lb-next"
        @click="store.lightboxNext()"
      >
        <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
          <path d="M7 4L13 10L7 16" stroke="white" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      </button>

      <!-- Thumbnail strip -->
      <div v-if="entries.length > 1" class="lb-strip">
        <button
          v-for="(entry, i) in entries"
          :key="entry.path"
          class="lb-thumb-btn"
          :class="{ active: i === index }"
          @click.stop="store.lightbox.index = i"
          :title="fileName(entry.path)"
        >
          <img
            v-if="!HEIC.has(entry.path.split('.').pop()?.toLowerCase())"
            :src="convertFileSrc(entry.path)"
            class="lb-thumb-img"
            draggable="false"
          />
          <img
            v-else-if="store.thumbCache[entry.path] && store.thumbCache[entry.path] !== '__error__'"
            :src="store.thumbCache[entry.path]"
            class="lb-thumb-img"
            draggable="false"
          />
          <div v-else class="lb-thumb-placeholder" />
          <span v-if="entry.isOriginal" class="lb-thumb-original" title="Original" />
          <span v-if="store.selected.has(entry.path)" class="lb-thumb-check">✓</span>
        </button>
      </div>

      <!-- Info bar -->
      <div class="lb-info">
        <span class="lb-name" :title="current.path">{{ fileName(current.path) }}</span>
        <span class="lb-meta">
          {{ current.width > 0 ? `${current.width}×${current.height}` : '—' }}
          · {{ formatSize(current.sizeBytes) }}
          · {{ formatDate(current.modified) }}
        </span>
        <span class="lb-counter">{{ index + 1 }} / {{ entries.length }}</span>
      </div>

    </div>
  </Transition>
</template>

<script setup>
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { convertFileSrc, invoke } from '@tauri-apps/api/core'
import { useScanStore } from '../store/scan'

const store   = useScanStore()
const HEIC    = new Set(['heic', 'heif'])
const entries = computed(() => store.lightbox?.entries ?? [])
const index   = computed(() => store.lightbox?.index ?? 0)
const current = computed(() => entries.value[index.value] ?? {})

// Full-res HEIC cache: path -> base64 data URL
const heicFullCache = ref({})
const imgEl = ref(null)
const badgeStyle = ref(null)

function updateBadgePos() {
  if (!imgEl.value) return
  const r = imgEl.value.getBoundingClientRect()
  const fr = imgEl.value.closest('.lb-frame')?.getBoundingClientRect()
  if (!fr) return
  badgeStyle.value = {
    top:  (r.top  - fr.top  + 10) + 'px',
    left: (r.left - fr.left + 10) + 'px',
  }
}

// Reset badge position when image changes
watch(current, () => { badgeStyle.value = null })

// For non-HEIC: convertFileSrc directly (full resolution, instant)
// For HEIC: loaded async via get_full_image, null while loading
const directSrc = computed(() => {
  const p = current.value?.path
  if (!p) return null
  const ext = p.split('.').pop()?.toLowerCase()
  if (HEIC.has(ext)) return heicFullCache.value[p] ?? null
  return convertFileSrc(p)
})

// Load full HEIC image when navigating to a HEIC entry
watch(current, async (entry) => {
  if (!entry?.path) return
  const ext = entry.path.split('.').pop()?.toLowerCase()
  if (!HEIC.has(ext)) return
  if (heicFullCache.value[entry.path]) return
  try {
    const src = await invoke('get_full_image', { path: entry.path })
    heicFullCache.value = { ...heicFullCache.value, [entry.path]: src }
  } catch {}
}, { immediate: true })

function onOverlayClick(e) {
  const keep = ['.lb-img', '.lb-nav', '.lb-strip', '.lb-thumb-btn', '.lb-close', '.lb-info']
  if (keep.some(sel => e.target.closest(sel))) return
  store.closeLightbox()
}

// Keyboard navigation
function onKeydown(e) {
  if (!store.lightbox) return
  if (e.key === 'ArrowRight' || e.key === 'ArrowDown') { e.preventDefault(); store.lightboxNext() }
  if (e.key === 'ArrowLeft'  || e.key === 'ArrowUp')   { e.preventDefault(); store.lightboxPrev() }
  if (e.key === 'Escape')                               { store.closeLightbox() }
  if (e.key === ' ') {
    e.preventDefault()
    const path = current.value?.path
    if (path) store.toggleSelected(path)
  }
}

onMounted(()        => window.addEventListener('keydown', onKeydown))
onBeforeUnmount(()  => window.removeEventListener('keydown', onKeydown))

// Formatters
function fileName(p) { return p?.split(/[/\\]/).pop() ?? '' }
function formatSize(b) {
  if (!b) return ''
  if (b < 1048576) return `${(b/1024).toFixed(1)} KB`
  return `${(b/1048576).toFixed(1)} MB`
}
function formatDate(iso) {
  if (!iso || iso.startsWith('1970')) return ''
  const d = new Date(iso)
  const pad = n => String(n).padStart(2, '0')
  return `${pad(d.getUTCDate())}/${pad(d.getUTCMonth()+1)}/${d.getUTCFullYear()} ${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}`
}
</script>

<style scoped>
.lb-overlay {
  position: fixed;
  inset: 0;
  z-index: 200;
  background: rgba(0, 0, 0, 0.92);
  display: flex;
  align-items: center;
  justify-content: center;
  flex-direction: column;
  gap: var(--space-2);
  padding: 48px 80px var(--space-3);
}

.lb-fade-enter-active, .lb-fade-leave-active { transition: opacity 150ms ease; }
.lb-fade-enter-from, .lb-fade-leave-to { opacity: 0; }

/* ── Close ── */
.lb-close {
  position: absolute;
  top: var(--space-4);
  right: var(--space-4);
  width: 36px;
  height: 36px;
  border-radius: 50%;
  background: rgba(255,255,255,0.1);
  color: #fff;
  font-size: 16px;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: background var(--transition);
  z-index: 10;
}
.lb-close:hover { background: rgba(255,255,255,0.2); }

/* ── Nav arrows ── */
.lb-nav {
  position: absolute;
  top: 50%;
  transform: translateY(-50%);
  width: 48px;
  height: 80px;
  background: rgba(255,255,255,0.08);
  color: #fff;
  font-size: 32px;
  line-height: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  border-radius: var(--border-radius-md);
  transition: background var(--transition);
  z-index: 10;
}
.lb-nav:hover { background: rgba(255,255,255,0.18); }
.lb-prev { left: var(--space-4); }
.lb-next { right: var(--space-4); }

/* ── Image frame ── */
.lb-frame {
  position: relative;
  flex: 1;
  width: calc(100vw - 160px);
  min-height: 0;
  display: flex;
  align-items: center;
  justify-content: center;
}

.lb-img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  border-radius: var(--border-radius-md);
  box-shadow: 0 8px 40px rgba(0,0,0,0.6);
}

.lb-img--thumb {
  /* Thumbnail shown while full image loads — upscaled smoothly */
  image-rendering: auto;
  filter: blur(1px);
}

.lb-badge {
  position: absolute;
  top: 10px;
  left: 10px;
  background: var(--color-success);
  color: #fff;
  font-size: 11px;
  font-weight: 600;
  padding: 3px 8px;
  border-radius: var(--border-radius-pill);
  pointer-events: none;
}

.lb-spinner {
  width: 40px;
  height: 40px;
  border: 3px solid rgba(255,255,255,0.2);
  border-top-color: #fff;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }

/* ── Info bar ── */
.lb-info {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-4);
  background: rgba(255,255,255,0.06);
  border-radius: var(--border-radius-md);
  max-width: calc(100vw - 160px);
  width: 100%;
  user-select: text;
  cursor: text;
}

.lb-name {
  font-size: var(--font-size-sm);
  color: #fff;
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  flex: 1;
}

.lb-meta {
  font-size: var(--font-size-xs);
  color: rgba(255,255,255,0.5);
  white-space: nowrap;
  flex-shrink: 0;
}

.lb-counter {
  font-size: var(--font-size-xs);
  color: rgba(255,255,255,0.4);
  white-space: nowrap;
  flex-shrink: 0;
}
/* ── Thumbnail strip ── */
.lb-strip {
  display: flex;
  gap: 6px;
  max-width: calc(100vw - 160px);
  overflow-x: auto;
  padding: 2px 4px;
  scrollbar-width: thin;
  scrollbar-color: rgba(255,255,255,0.2) transparent;
}

.lb-thumb-btn {
  position: relative;
  flex-shrink: 0;
  width: 56px;
  height: 56px;
  border-radius: var(--border-radius-sm);
  overflow: hidden;
  border: 2px solid transparent;
  opacity: 0.5;
  transition: opacity var(--transition), border-color var(--transition);
  background: rgba(255,255,255,0.05);
}

.lb-thumb-btn:hover { opacity: 0.8; }
.lb-thumb-btn.active {
  border-color: var(--color-accent);
  opacity: 1;
}

.lb-thumb-img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}

.lb-thumb-placeholder {
  width: 100%;
  height: 100%;
  background: rgba(255,255,255,0.05);
}

.lb-thumb-original {
  position: absolute;
  top: 3px;
  left: 3px;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--color-success);
  border: 1.5px solid rgba(0,0,0,0.4);
  pointer-events: none;
}

.lb-thumb-check {
  position: absolute;
  inset: 0;
  background: rgba(74, 144, 217, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 18px;
  color: #fff;
  font-weight: 700;
  pointer-events: none;
}
</style>
