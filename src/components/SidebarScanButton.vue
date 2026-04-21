<template>
  <button
    class="btn-scan"
    :class="scanning ? 'btn-danger' : 'btn-success'"
    :disabled="!scanning && disabled"
    @click="scanning ? emit('stop') : emit('start')"
  >
    {{ scanning ? stopLabel : startLabel }}
  </button>
</template>

<script setup>
defineProps({
  scanning:   { type: Boolean, default: false },
  disabled:   { type: Boolean, default: false },
  startLabel: { type: String,  default: 'Scan' },
  stopLabel:  { type: String,  default: 'Stop scan' },
})
const emit = defineEmits(['start', 'stop'])
</script>
