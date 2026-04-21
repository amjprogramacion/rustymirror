<template>
  <div>
    <button v-if="collapsible" class="mp-section-title" @click="emit('toggle')">
      <slot name="title" /> <ChevronIcon :open="!collapsed" />
    </button>
    <div v-else class="mp-section-title">
      <slot name="title" />
    </div>

    <div v-show="!collapsible || !collapsed">
      <slot />
    </div>
  </div>
</template>

<script setup>
import ChevronIcon from './ChevronIcon.vue'

defineProps({
  collapsible: { type: Boolean, default: false },
  collapsed:   { type: Boolean, default: false },
})
const emit = defineEmits(['toggle'])
</script>
