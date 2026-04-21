<template>
  <CollapsibleSection :collapsible="collapsible" :collapsed="collapsed" @toggle="emit('toggle')">
    <template #title>Exposure</template>

    <div class="ps-rows">
      <template v-for="row in exposureRows" :key="row.label">
        <div class="ps-row" v-if="row.visible">
          <span class="ps-label">{{ row.label }}</span>
          <span class="ps-value" v-if="!isBatch || row.value !== MIXED_VALUE">{{ row.value }}</span>
          <span class="ps-value ps-value--muted" v-else>Various values</span>
        </div>
      </template>
    </div>
  </CollapsibleSection>
</template>

<script setup>
import { computed } from 'vue'
import CollapsibleSection from './CollapsibleSection.vue'
import { MIXED_VALUE } from '../constants'
import '../styles/panel-sections.css'

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
