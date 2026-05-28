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
          <div class="slm-coords">
            <label class="slm-field">
              <span class="slm-field-label">Latitude</span>
              <input
                v-model="lat"
                class="slm-input"
                type="text"
                placeholder="40.71600"
                @keydown.enter.prevent="onSave"
                @keydown.esc.prevent="$emit('close')"
              />
            </label>
            <label class="slm-field">
              <span class="slm-field-label">Longitude</span>
              <input
                v-model="lon"
                class="slm-input"
                type="text"
                placeholder="-74.00600"
                @keydown.enter.prevent="onSave"
                @keydown.esc.prevent="$emit('close')"
              />
            </label>
          </div>
          <p v-if="duplicate" class="slm-error">
            A custom location (“{{ duplicate.name }}”) already exists at these coordinates.
          </p>
          <div class="slm-actions">
            <button class="slm-btn slm-btn-ghost" @click="$emit('close')">Cancel</button>
            <button class="slm-btn slm-btn-primary" :disabled="!valid" @click="onSave">Save</button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup>
import { ref, computed, nextTick, watch } from 'vue'

const props = defineProps({
  show:     { type: Boolean, default: false },
  lat:      { type: Number,  default: null },
  lon:      { type: Number,  default: null },
  existing: { type: Array,   default: () => [] }, // saved locations, for duplicate check
})
const emit = defineEmits(['save', 'close'])

// Two presets within ~11 m (0.0001°) are treated as the same place.
const DUP_EPS = 0.0001

const name    = ref('')
const lat     = ref('')
const lon     = ref('')
const inputEl = ref(null)

const coordsValid = computed(() => {
  const la = parseFloat(lat.value)
  const lo = parseFloat(lon.value)
  return !Number.isNaN(la) && la >= -90  && la <= 90
      && !Number.isNaN(lo) && lo >= -180 && lo <= 180
})

// Existing preset whose coordinates match (within tolerance) the entered ones.
const duplicate = computed(() => {
  if (!coordsValid.value) return null
  const la = parseFloat(lat.value)
  const lo = parseFloat(lon.value)
  return props.existing.find(l =>
    Math.abs(l.lat - la) < DUP_EPS && Math.abs(l.lon - lo) < DUP_EPS) ?? null
})

const valid = computed(() => !!name.value.trim() && coordsValid.value && !duplicate.value)

watch(() => props.show, (open) => {
  if (open) {
    name.value = ''
    lat.value  = props.lat != null ? props.lat.toFixed(6) : ''
    lon.value  = props.lon != null ? props.lon.toFixed(6) : ''
    nextTick(() => inputEl.value?.focus())
  }
})

function onSave() {
  if (!valid.value) return
  emit('save', {
    name: name.value.trim(),
    lat:  parseFloat(lat.value),
    lon:  parseFloat(lon.value),
  })
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
.slm-coords {
  display: flex;
  gap: var(--space-2);
  margin-top: var(--space-2);
}
.slm-field {
  flex: 1 1 0;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.slm-field-label {
  font-size: 10px;
  color: var(--text-muted);
}
.slm-error {
  margin: var(--space-3) 0 0;
  font-size: var(--font-size-xs);
  color: var(--color-danger);
  line-height: 1.3;
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
