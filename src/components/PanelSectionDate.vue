<template>
  <div>
    <button v-if="collapsible" class="mp-section-title" @click="emit('toggle')">
      Date taken <ChevronIcon :open="!collapsed" />
    </button>
    <div v-else class="mp-section-title">Date taken</div>

    <div class="ps-edit-rows" v-show="!collapsible || !collapsed">
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
  </div>
</template>

<script setup>
import ChevronIcon from './ChevronIcon.vue'
import { isoToDatetimeLocal, datetimeLocalToIso } from '../utils/formatters'

defineProps({
  value:       String,
  isMixed:     { type: Boolean, default: false },
  showHint:    { type: Boolean, default: false },
  collapsible: { type: Boolean, default: false },
  collapsed:   { type: Boolean, default: false },
})
const emit = defineEmits(['toggle', 'change'])
</script>

<style scoped>
/* .mp-section-title is styled via :deep() in ImageDetailPanel */

.ps-edit-rows {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding-bottom: var(--space-2);
}

.ps-edit-row {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.ps-input {
  width: 100%;
  padding: 5px 8px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-primary);
  font-size: var(--font-size-xs);
  outline: none;
  transition: border-color var(--transition);
  box-sizing: border-box;
}
.ps-input:focus { border-color: var(--color-accent); }
.ps-input::placeholder { color: var(--text-muted); }

.ps-input[type="datetime-local"]::-webkit-calendar-picker-indicator {
  filter: invert(0.6);
  cursor: pointer;
}

.ps-hint {
  font-size: 10px;
  color: var(--text-muted);
  font-style: italic;
}
</style>
