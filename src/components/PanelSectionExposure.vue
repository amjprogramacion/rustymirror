<template>
  <div>
    <button v-if="collapsible" class="mp-section-title" @click="emit('toggle')">
      Exposure <ChevronIcon :open="!collapsed" />
    </button>
    <div v-else class="mp-section-title">Exposure</div>

    <div class="ps-rows" v-show="!collapsible || !collapsed">
      <template v-for="row in exposureRows" :key="row.label">
        <div class="ps-row" v-if="row.visible">
          <span class="ps-label">{{ row.label }}</span>
          <span class="ps-value" v-if="!isBatch || row.value !== MIXED">{{ row.value }}</span>
          <span class="ps-value ps-value--muted" v-else>Various values</span>
        </div>
      </template>
    </div>
  </div>
</template>

<script setup>
import { computed } from 'vue'
import ChevronIcon from './ChevronIcon.vue'

const MIXED = '__mixed__'

const props = defineProps({
  meta:        Object,
  batchAgg:    Object,
  isBatch:     { type: Boolean, default: false },
  collapsible: { type: Boolean, default: false },
  collapsed:   { type: Boolean, default: true },
})
const emit = defineEmits(['toggle'])

const exposureRows = computed(() => {
  const a = props.batchAgg
  const m = props.meta
  function row(label, singleVal, batchField) {
    const bv = a?.[batchField]
    const visible = props.isBatch ? (bv != null) : !!singleVal
    const value   = props.isBatch ? bv : singleVal
    return { label, value, visible }
  }
  return [
    row('Shutter',       m?.exposureTime, 'exposureTime'),
    row('Aperture',      m?.fNumber,      'fNumber'),
    row('ISO',           m?.isoSpeed,     'isoSpeed'),
    row('Focal length',  m?.focalLength,  'focalLength'),
    row('Flash',         m?.flash,        'flash'),
    row('White balance', m?.whiteBalance, 'whiteBalance'),
    row('Exp. mode',     m?.exposureMode, 'exposureMode'),
    row('Metering',      m?.meteringMode, 'meteringMode'),
  ]
})
</script>

<style scoped>
/* .mp-section-title is styled via :deep() in ImageDetailPanel */

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
