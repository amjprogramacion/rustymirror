<template>
  <Teleport to="body">
    <Transition name="slm-fade">
      <div v-if="show" class="slm-overlay" @click.self="$emit('close')">
        <div class="slm-card" role="dialog" aria-modal="true" aria-label="Save custom location">
          <div class="slm-title">Save as custom location</div>
          <input
            ref="inputEl"
            v-model="name"
            class="slm-input"
            type="text"
            placeholder="Location name"
            maxlength="60"
            @keydown.enter.prevent="onSave"
            @keydown.esc.prevent="$emit('close')"
          />
          <div class="slm-actions">
            <button class="slm-btn slm-btn-ghost" @click="$emit('close')">Cancel</button>
            <button class="slm-btn slm-btn-primary" :disabled="!name.trim()" @click="onSave">Save</button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup>
import { ref, nextTick, watch } from 'vue'

const props = defineProps({
  show: { type: Boolean, default: false },
})
const emit = defineEmits(['save', 'close'])

const name    = ref('')
const inputEl = ref(null)

watch(() => props.show, (open) => {
  if (open) {
    name.value = ''
    nextTick(() => inputEl.value?.focus())
  }
})

function onSave() {
  const trimmed = name.value.trim()
  if (!trimmed) return
  emit('save', trimmed)
}
</script>

<style scoped>
.slm-overlay {
  position: fixed;
  inset: 0;
  z-index: 2000;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.55);
}
.slm-card {
  width: 320px;
  max-width: 90vw;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-md);
  padding: var(--space-4);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
}
.slm-title {
  font-size: var(--font-size-md);
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: var(--space-3);
}
.slm-input {
  width: 100%;
  box-sizing: border-box;
  padding: 8px 10px;
  font-size: var(--font-size-sm);
  color: var(--text-primary);
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  outline: none;
}
.slm-input:focus {
  border-color: var(--color-accent);
}
.slm-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2);
  margin-top: var(--space-4);
}
.slm-btn {
  padding: 6px 14px;
  font-size: var(--font-size-sm);
  font-weight: 600;
  border-radius: var(--border-radius-sm);
  border: 1px solid var(--border-color);
  cursor: pointer;
  transition: background 0.15s, border-color 0.15s, opacity 0.15s;
}
.slm-btn-ghost {
  background: transparent;
  color: var(--text-secondary);
}
.slm-btn-ghost:hover {
  background: var(--bg-card-hover);
}
.slm-btn-primary {
  background: var(--color-accent);
  border-color: var(--color-accent);
  color: #fff;
}
.slm-btn-primary:hover {
  background: var(--color-accent-hover);
  border-color: var(--color-accent-hover);
}
.slm-btn-primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.slm-fade-enter-active,
.slm-fade-leave-active {
  transition: opacity 0.15s ease;
}
.slm-fade-enter-from,
.slm-fade-leave-to {
  opacity: 0;
}
</style>
