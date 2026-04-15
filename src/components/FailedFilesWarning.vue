<template>
  <div v-if="files.length > 0 && !dismissed" class="failed-warning">
    <div class="failed-warning-header">
      <span class="failed-warning-label">
        {{ files.length }} {{ files.length === 1 ? 'file' : 'files' }} could not be read
      </span>
      <div class="failed-warning-actions">
        <button class="failed-btn" @click="expanded = !expanded">
          {{ expanded ? 'Hide details' : 'Show details' }}
        </button>
        <button class="failed-btn failed-btn-dismiss" @click="dismissed = true" title="Dismiss">✕</button>
      </div>
    </div>
    <ul v-if="expanded" class="failed-list">
      <li v-for="f in files" :key="f.path" class="failed-item">
        <span class="failed-path" :title="f.path">{{ basename(f.path) }}</span>
        <span class="failed-reason">{{ kindLabel(f.kind) }}</span>
      </li>
    </ul>
  </div>
</template>

<script setup>
import { ref, watch } from 'vue'

const props = defineProps({
  files: { type: Array, default: () => [] },
})

const dismissed = ref(false)
const expanded = ref(false)

// Reset when files change (new scan)
watch(() => props.files, () => {
  dismissed.value = false
  expanded.value = false
})

const KIND_LABELS = {
  PERMISSION_DENIED:  'Permission denied',
  NOT_FOUND:          'File not found',
  UNSUPPORTED_FORMAT: 'Unsupported format',
  CORRUPTED_FILE:     'Corrupted file',
  IO_ERROR:           'I/O error',
}

function kindLabel(kind) {
  return KIND_LABELS[kind] ?? 'Unknown error'
}

function basename(path) {
  return path.replace(/\\/g, '/').split('/').pop() || path
}
</script>

<style scoped>
.failed-warning {
  margin: 0 0 12px;
  border: 1px solid #c0822a;
  border-radius: var(--border-radius-sm);
  background: rgba(192, 130, 42, 0.08);
  font-size: var(--font-size-xs);
  overflow: hidden;
}

.failed-warning-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 7px 10px;
}

.failed-warning-label {
  color: #c9933a;
  font-weight: 500;
}

.failed-warning-actions {
  display: flex;
  gap: 6px;
  flex-shrink: 0;
}

.failed-btn {
  background: transparent;
  border: 1px solid rgba(192, 130, 42, 0.4);
  border-radius: var(--border-radius-sm);
  color: #c9933a;
  cursor: pointer;
  font-size: var(--font-size-xs);
  padding: 2px 8px;
  transition: background 0.15s;
}
.failed-btn:hover {
  background: rgba(192, 130, 42, 0.15);
}
.failed-btn-dismiss {
  padding: 2px 6px;
  border-color: transparent;
  color: var(--text-muted);
}
.failed-btn-dismiss:hover {
  color: var(--text-secondary);
  background: var(--bg-card);
}

.failed-list {
  list-style: none;
  margin: 0;
  padding: 0 10px 8px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 200px;
  overflow-y: auto;
}

.failed-item {
  display: flex;
  gap: 8px;
  align-items: baseline;
}

.failed-path {
  color: var(--text-secondary);
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 240px;
  flex-shrink: 0;
}

.failed-reason {
  color: var(--text-muted);
}
</style>
