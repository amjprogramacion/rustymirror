<template>
  <button
    class="btn-scan"
    :class="scanning ? 'btn-danger' : 'btn-success'"
    :disabled="stopping || (!scanning && disabled)"
    @click="scanning && !stopping ? emit('stop') : (!scanning ? emit('start') : null)"
  >
    {{ stopping ? stoppingLabel : (scanning ? stopLabel : startLabel) }}
  </button>
</template>

<script setup>
defineProps({
  scanning:      { type: Boolean, default: false },
  stopping:      { type: Boolean, default: false },
  disabled:      { type: Boolean, default: false },
  startLabel:    { type: String,  default: 'Scan' },
  stopLabel:     { type: String,  default: 'Stop scan' },
  stoppingLabel: { type: String,  default: 'Stopping...' },
})
const emit = defineEmits(['start', 'stop'])
</script>
