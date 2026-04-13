<template>
  <section class="sidebar-section">
    <p class="section-label">Folders</p>
    <button class="btn btn-secondary btn-full" @click="$emit('add')">+ Add folder</button>
    <ul class="folder-list" v-if="folders.length">
      <li v-for="folder in folders" :key="folder" class="folder-item">
        <span class="folder-path" :title="folder">{{ folder }}</span>
        <button class="btn-remove" @click="$emit('remove', folder)" title="Remove folder">✕</button>
      </li>
    </ul>
  </section>
</template>

<script setup>
defineProps({
  folders: {
    type: Array,
    required: true,
  },
})

defineEmits(['add', 'remove'])
</script>

<style scoped>
/* ── Add folder button ── */
.btn {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 7px var(--space-3);
  border-radius: var(--border-radius-md);
  font-size: var(--font-size-sm);
  font-weight: 500;
  transition: background var(--transition), color var(--transition),
              border-color var(--transition), opacity var(--transition);
}
.btn-full { width: 100%; }
.btn-secondary {
  background: var(--bg-card);
  color: var(--text-secondary);
  border: 1px solid var(--border-color);
}
.btn-secondary:hover:not(:disabled) { background: var(--bg-card-hover); }

/* ── Layout ── */
.sidebar-section {
  padding: var(--space-3);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.section-label {
  font-size: var(--font-size-xs);
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.6px;
  display: flex;
  align-items: center;
}

.folder-list {
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.folder-item {
  display: flex;
  align-items: flex-start;
  gap: var(--space-1);
  padding: 4px var(--space-1);
  border-radius: var(--border-radius-sm);
  background: var(--bg-card);
}

.folder-path {
  flex: 1;
  font-size: var(--font-size-xs);
  color: var(--text-secondary);
  word-break: break-all;
  line-height: 1.4;
}

.btn-remove {
  background: none;
  color: var(--color-danger);
  font-size: 11px;
  padding: 0 2px;
  line-height: 1;
  flex-shrink: 0;
  margin-top: 1px;
  transition: color var(--transition);
}
.btn-remove:hover { color: var(--color-danger-hover); }
</style>
