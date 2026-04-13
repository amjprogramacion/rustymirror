<template>
  <div class="search-wrap">
    <span class="search-icon">⌕</span>
    <input
      class="search-input"
      type="text"
      :placeholder="placeholder"
      :value="modelValue"
      @input="$emit('update:modelValue', $event.target.value)"
      @keydown.escape="$emit('update:modelValue', '')"
    />
    <button
      v-if="modelValue"
      class="search-clear"
      @click="$emit('update:modelValue', '')"
      title="Clear search"
    >✕</button>
  </div>
</template>

<script setup>
defineProps({
  modelValue: {
    type: String,
    default: '',
  },
  placeholder: {
    type: String,
    default: 'Search by filename…',
  },
})

defineEmits(['update:modelValue'])
</script>

<style scoped>
.search-wrap {
  margin-left: auto;
  position: relative;
  display: flex;
  align-items: center;
}

.search-icon {
  position: absolute;
  left: 8px;
  font-size: 15px;
  color: var(--text-muted);
  pointer-events: none;
  line-height: 1;
}

.search-input {
  width: 200px;
  padding: 4px 28px 4px 26px;
  border-radius: var(--border-radius-md);
  border: 1px solid var(--border-color);
  background: var(--bg-card);
  color: var(--text-primary);
  font-size: var(--font-size-sm);
  transition: border-color var(--transition), width var(--transition);
  outline: none;
}

.search-input::placeholder { color: var(--text-muted); }

.search-input:focus {
  border-color: var(--color-accent);
  width: 260px;
}

.search-clear {
  position: absolute;
  right: 6px;
  background: none;
  color: var(--text-muted);
  font-size: 11px;
  padding: 2px 4px;
  line-height: 1;
  border-radius: 3px;
  transition: color var(--transition);
}
.search-clear:hover { color: var(--text-primary); }
</style>
