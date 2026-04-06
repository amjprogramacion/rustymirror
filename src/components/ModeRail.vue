<template>
  <nav class="mode-rail">
    <button
      v-for="mode in modes"
      :key="mode.id"
      class="rail-btn"
      :class="{ active: activeMode === mode.id }"
      :title="mode.label"
      @click="activeMode = mode.id"
    >
      <component :is="mode.icon" />
    </button>
  </nav>
</template>

<script setup>
import { useMode } from '../composables/useMode'
import { defineComponent, h } from 'vue'

const { activeMode } = useMode()

const IconDuplicates = defineComponent({
  render: () => h('svg', { xmlns: 'http://www.w3.org/2000/svg', width: 18, height: 18, viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', 'stroke-width': 2, 'stroke-linecap': 'round', 'stroke-linejoin': 'round' }, [
    h('rect', { x: 8, y: 8, width: 13, height: 13, rx: 2 }),
    h('path', { d: 'M4 16V6a2 2 0 0 1 2-2h10' }),
  ]),
})

const IconMetadata = defineComponent({
  render: () => h('svg', { xmlns: 'http://www.w3.org/2000/svg', width: 18, height: 18, viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', 'stroke-width': 2, 'stroke-linecap': 'round', 'stroke-linejoin': 'round' }, [
    h('path', { d: 'M12 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z' }),
    h('polyline', { points: '12 2 12 8 18 8' }),
    h('line', { x1: 8, y1: 13, x2: 16, y2: 13 }),
    h('line', { x1: 8, y1: 17, x2: 16, y2: 17 }),
  ]),
})

const modes = [
  { id: 'duplicates', label: 'Duplicate finder', icon: IconDuplicates },
  { id: 'metadata',   label: 'Metadata editor',  icon: IconMetadata   },
]
</script>

<style scoped>
.mode-rail {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 6px 0;
  gap: var(--space-2);
  width: 44px;
  min-width: 44px;
  background: var(--bg-primary);
  border-right: 1px solid var(--sidebar-border);
  height: 100%;
}

.rail-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border-radius: var(--border-radius-sm);
  background: none;
  color: var(--text-muted);
  transition: color var(--transition), background var(--transition);
}

.rail-btn:hover {
  color: var(--text-secondary);
  background: var(--bg-card);
}

.rail-btn.active {
  color: var(--color-accent);
  background: var(--bg-card);
}
</style>
