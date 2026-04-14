<template>
  <div>
    <button v-if="collapsible" class="ps-section-title" @click="emit('toggle')">
      File &amp; Camera <ChevronIcon :open="!collapsed" />
    </button>
    <div v-else class="ps-section-title">File &amp; Camera</div>

    <div class="ps-rows" v-show="!collapsible || !collapsed">
      <div class="ps-row">
        <span class="ps-label">Size</span>
        <template v-if="isBatch && batchAgg">
          <span class="ps-value" v-if="batchAgg.fileSize !== MIXED">{{ formatSize(batchAgg.fileSize) }}</span>
          <span class="ps-value ps-value--muted" v-else>Various values</span>
        </template>
        <span class="ps-value" v-else-if="meta">{{ formatSize(meta.fileSize) }}</span>
      </div>

      <div class="ps-row" v-if="showDims">
        <span class="ps-label">Dims</span>
        <template v-if="isBatch && batchAgg">
          <span class="ps-value" v-if="batchAgg.width !== MIXED">{{ batchAgg.width }}×{{ batchAgg.height }}</span>
          <span class="ps-value ps-value--muted" v-else>Various values</span>
        </template>
        <span class="ps-value" v-else-if="meta">{{ meta.width }}×{{ meta.height }}</span>
      </div>

      <div class="ps-row" v-if="showDevice">
        <span class="ps-label">Device</span>
        <template v-if="isBatch && batchAgg">
          <span class="ps-value" v-if="batchAgg.make !== MIXED && batchAgg.model !== MIXED">{{ [batchAgg.make, batchAgg.model].filter(Boolean).join(' ') }}</span>
          <span class="ps-value ps-value--muted" v-else>Various values</span>
        </template>
        <span class="ps-value" v-else-if="meta">{{ [meta.make, meta.model].filter(Boolean).join(' ') }}</span>
      </div>

      <div class="ps-row" v-if="showLens">
        <span class="ps-label">Lens</span>
        <template v-if="isBatch && batchAgg">
          <span class="ps-value" v-if="batchAgg.lensModel !== MIXED">{{ batchAgg.lensModel }}</span>
          <span class="ps-value ps-value--muted" v-else>Various values</span>
        </template>
        <span class="ps-value" v-else-if="meta">{{ meta.lensModel }}</span>
      </div>

      <div class="ps-row" v-if="showSoftware">
        <span class="ps-label">Software</span>
        <template v-if="isBatch && batchAgg">
          <span class="ps-value" v-if="batchAgg.software !== MIXED">{{ batchAgg.software }}</span>
          <span class="ps-value ps-value--muted" v-else>Various values</span>
        </template>
        <span class="ps-value" v-else-if="meta">{{ meta.software }}</span>
      </div>
    </div>
  </div>
</template>

<script setup>
import { computed } from 'vue'
import ChevronIcon from './ChevronIcon.vue'
import { formatSize } from '../utils/formatters'

const MIXED = '__mixed__'

const props = defineProps({
  meta:        Object,
  batchAgg:    Object,
  isBatch:     { type: Boolean, default: false },
  collapsible: { type: Boolean, default: false },
  collapsed:   { type: Boolean, default: false },
})
const emit = defineEmits(['toggle'])

const showDims    = computed(() => props.isBatch ? (props.batchAgg?.width != null) : (props.meta?.width > 0))
const showDevice  = computed(() => props.isBatch ? (props.batchAgg?.make != null || props.batchAgg?.model != null) : !!(props.meta?.make || props.meta?.model))
const showLens    = computed(() => props.isBatch ? (props.batchAgg?.lensModel != null) : !!props.meta?.lensModel)
const showSoftware = computed(() => props.isBatch ? (props.batchAgg?.software != null) : !!props.meta?.software)
</script>

<style scoped>
.ps-section-title {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 10px;
  font-weight: 700;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.6px;
  white-space: nowrap;
  background: none;
  border: none;
  padding: 0;
  text-align: left;
  line-height: 1;
  flex-shrink: 0;
}
button.ps-section-title {
  width: 100%;
  height: 36px;
  cursor: pointer;
}
button.ps-section-title:hover { color: var(--text-secondary); }

.ps-rows {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding-bottom: var(--space-2);
}

.ps-row {
  display: flex;
  align-items: baseline;
  gap: var(--space-2);
  font-size: var(--font-size-xs);
}

.ps-label {
  color: var(--text-muted);
  flex-shrink: 0;
  width: 80px;
}

.ps-value {
  color: var(--text-secondary);
  word-break: break-word;
  font-size: var(--font-size-xs);
}

.ps-value--muted {
  color: var(--text-muted);
  font-style: italic;
}
</style>
