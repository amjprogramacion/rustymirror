<template>
  <CollapsibleSection :collapsible="collapsible" :collapsed="collapsed" @toggle="emit('toggle')">
    <template #title>Date taken</template>

    <div class="ps-edit-rows">
      <label class="ps-edit-row">
        <input
          class="ps-input"
          type="datetime-local"
          step="1"
          :value="isoToDatetimeLocal(value)"
          :placeholder="isMixed ? 'Various values' : ''"
          @change="e => emit('change', datetimeLocalToIso(e.target.value))"
        />
      </label>
      <p v-if="showHint" class="ps-hint">Various values — leave empty to keep each file's date</p>
    </div>
  </CollapsibleSection>
</template>

<script setup>
import CollapsibleSection from './CollapsibleSection.vue'
import { isoToDatetimeLocal, datetimeLocalToIso } from '../utils/formatters'
import '../styles/panel-sections.css'

defineProps({
  value:       String,
  isMixed:     { type: Boolean, default: false },
  showHint:    { type: Boolean, default: false },
  collapsible: { type: Boolean, default: false },
  collapsed:   { type: Boolean, default: false },
})
const emit = defineEmits(['toggle', 'change'])
</script>
