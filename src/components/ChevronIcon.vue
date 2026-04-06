<template>
  <svg
    class="chevron-icon"
    :class="{ 'chevron-icon--open': open }"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2.5"
    stroke-linecap="round"
    stroke-linejoin="round"
  >
    <polyline points="6 9 12 15 18 9" />
  </svg>
</template>

<script setup>
defineProps({
  open: { type: Boolean, default: false },
})
</script>

<style scoped>
.chevron-icon {
  width: 20px;
  height: 20px;
  color: var(--text-muted);
  transform: rotate(0deg);
  transform-origin: center center;
  transition: transform 180ms ease;
  flex-shrink: 0;
}
.chevron-icon--open {
  transform: rotate(180deg);
}
</style>
