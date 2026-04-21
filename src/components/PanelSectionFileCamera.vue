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

      <div class="ps-row" v-if="showDevice">
        <span class="ps-label">Device</span>
        <template v-if="isBatch && batchAgg">
          <span class="ps-value" v-if="batchAgg.make !== MIXED_VALUE && batchAgg.model !== MIXED_VALUE">{{ [batchAgg.make, batchAgg.model].filter(Boolean).join(' ') }}</span>
          <span class="ps-value ps-value--muted" v-else>Various values</span>
        </template>
        <span class="ps-value" v-else-if="meta">{{ [meta.make, meta.model].filter(Boolean).join(' ') }}</span>
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
    </div>
  </CollapsibleSection>
</template>

<script setup>
import { computed } from 'vue'
import CollapsibleSection from './CollapsibleSection.vue'
import { formatSize } from '../utils/formatters'
import { MIXED_VALUE } from '../constants'
import '../styles/panel-sections.css'

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
