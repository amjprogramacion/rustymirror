<template>
  <CollapsibleSection :collapsible="collapsible" :collapsed="collapsed" @toggle="emit('toggle')">
    <template #title>File &amp; Camera</template>

    <div class="ps-rows">
      <div class="ps-row">
        <span class="ps-label">Size</span>
        <template v-if="isBatch && batchAgg">
          <span class="ps-value" v-if="batchAgg.fileSize !== MIXED_VALUE">{{ formatSize(batchAgg.fileSize) }}</span>
          <span class="ps-value ps-value--muted" v-else>Various values</span>
        </template>
        <span class="ps-value" v-else-if="meta">{{ formatSize(meta.fileSize) }}</span>
      </div>

      <div class="ps-row" v-if="showDims">
        <span class="ps-label">Dims</span>
        <template v-if="isBatch && batchAgg">
          <span class="ps-value" v-if="batchAgg.width !== MIXED_VALUE">{{ batchAgg.width }}×{{ batchAgg.height }}</span>
          <span class="ps-value ps-value--muted" v-else>Various values</span>
        </template>
        <span class="ps-value" v-else-if="meta">{{ meta.width }}×{{ meta.height }}</span>
      </div>

      <div class="ps-row" v-if="showLens">
        <span class="ps-label">Lens</span>
        <template v-if="isBatch && batchAgg">
          <span class="ps-value" v-if="batchAgg.lensModel !== MIXED_VALUE">{{ batchAgg.lensModel }}</span>
          <span class="ps-value ps-value--muted" v-else>Various values</span>
        </template>
        <span class="ps-value" v-else-if="meta">{{ meta.lensModel }}</span>
      </div>

      <div class="ps-row" v-if="showSoftware">
        <span class="ps-label">Software</span>
        <template v-if="isBatch && batchAgg">
          <span class="ps-value" v-if="batchAgg.software !== MIXED_VALUE">{{ batchAgg.software }}</span>
          <span class="ps-value ps-value--muted" v-else>Various values</span>
        </template>
        <span class="ps-value" v-else-if="meta">{{ meta.software }}</span>
      </div>

      <!-- Device row -->
      <div class="ps-row ps-row--device">
        <span class="ps-label">Device</span>

        <template v-if="editing && canEditDevice">
          <select class="ps-input ps-device-select" :value="selectValue" @change="onDeviceSelect" @blur="editing = false">
            <option v-if="editDevice === null" value="__mixed__" disabled>Various values</option>
            <option v-for="d in allDevices" :key="d" :value="d">{{ metaStore.deviceAliases[d] || d }}</option>
          </select>
        </template>
        <template v-else>
          <span class="ps-value" v-if="editDevice">{{ metaStore.deviceAliases[editDevice] || editDevice }}</span>
          <span class="ps-value ps-value--muted" v-else-if="editDevice === null">Various values</span>
          <span class="ps-value ps-value--muted" v-else>-</span>
        </template>

        <div class="ps-device-actions" v-if="canEditDevice">
          <button class="ps-device-btn" :class="{ active: editing }" @click="editing = !editing" title="Change device">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/>
              <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>
            </svg>
          </button>
          <button
            v-if="canDeleteDevice"
            class="ps-device-btn ps-device-btn--remove"
            @click="onDeleteDevice"
            title="Remove device"
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <line x1="18" y1="6" x2="6" y2="18"/>
              <line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>
    </div>
  </CollapsibleSection>
</template>

<script setup>
import { ref, computed } from 'vue'
import CollapsibleSection from './CollapsibleSection.vue'
import { useMetadataStore } from '../store/metadata'
import { formatSize } from '../utils/formatters'
import { MIXED_VALUE } from '../constants'
import '../styles/panel-sections.css'

const metaStore = useMetadataStore()

const props = defineProps({
  meta:        Object,
  batchAgg:    Object,
  isBatch:     { type: Boolean, default: false },
  editDevice:  { type: String, default: '' },  // null = batch-mixed not yet overridden
  collapsible: { type: Boolean, default: false },
  collapsed:   { type: Boolean, default: false },
})
const emit = defineEmits(['toggle', 'update:device', 'change'])

const editing = ref(false)
const canEditDevice = computed(() => props.editDevice !== null || props.isBatch)
const canDeleteDevice = computed(() => !!props.editDevice || (props.isBatch && props.editDevice === null))

// Combine custom + discovered, deduped and sorted. Add current value if missing.
const allDevices = computed(() => {
  const seen = new Set()
  const result = []
  for (const name of [...metaStore.discoveredDevices, ...metaStore.customDevices]) {
    if (!seen.has(name)) { seen.add(name); result.push(name) }
  }
  if (props.editDevice && !seen.has(props.editDevice)) {
    result.push(props.editDevice)
  }
  return result.sort((a, b) => a.localeCompare(b, undefined, { sensitivity: 'base' }))
})

const selectValue = computed(() => props.editDevice === null ? '__mixed__' : (props.editDevice ?? ''))

function onDeviceSelect(e) {
  editing.value = false
  emit('update:device', e.target.value)
  emit('change')
}

function onDeleteDevice() {
  editing.value = false
  emit('update:device', '')
  emit('change')
}

const showDims     = computed(() => props.isBatch ? (props.batchAgg?.width != null) : (props.meta?.width > 0))
const showLens     = computed(() => props.isBatch ? (props.batchAgg?.lensModel != null) : !!props.meta?.lensModel)
const showSoftware = computed(() => props.isBatch ? (props.batchAgg?.software != null) : !!props.meta?.software)
</script>

<style scoped>
.ps-row--device {
  align-items: center;
}

.ps-device-select {
  flex: 1;
  min-width: 0;
  padding: 3px 6px;
  font-size: var(--font-size-xs);
}

.ps-row--device .ps-value,
.ps-row--device .ps-value--muted {
  flex: 1;
  min-width: 0;
}

.ps-device-actions {
  display: flex;
  gap: 0;
  flex-shrink: 0;
}

.ps-device-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  padding: 0;
  background: transparent;
  border: 1px solid transparent;
  border-radius: var(--border-radius-sm);
  color: var(--text-muted);
  cursor: pointer;
  transition: color var(--transition), border-color var(--transition), background var(--transition);
}

.ps-device-btn:hover,
.ps-device-btn.active {
  color: var(--text-primary);
  background: var(--bg-hover);
  border-color: var(--border-color);
}

.ps-device-btn--remove:hover:not(:disabled) {
  color: var(--color-danger, #e05050);
  background: var(--bg-hover);
  border-color: var(--color-danger, #e05050);
}

.ps-device-btn:disabled {
  opacity: 0.3;
  cursor: default;
}
</style>
