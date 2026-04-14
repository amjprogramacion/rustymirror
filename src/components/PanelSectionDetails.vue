<template>
  <div>
    <button v-if="collapsible" class="ps-section-title" @click="emit('toggle')">
      Details <ChevronIcon :open="!collapsed" />
    </button>
    <div v-else class="ps-section-title">Details</div>

    <div class="ps-edit-rows" v-show="!collapsible || !collapsed">
      <label class="ps-edit-row">
        <span class="ps-label">Description</span>
        <input
          class="ps-input"
          type="text"
          :value="description"
          @input="e => { emit('update:description', e.target.value); emit('change') }"
          :placeholder="descMixed ? 'Various values' : 'Add a description…'"
        />
      </label>
      <label class="ps-edit-row">
        <span class="ps-label">Artist</span>
        <input
          class="ps-input"
          type="text"
          :value="artist"
          @input="e => { emit('update:artist', e.target.value); emit('change') }"
          :placeholder="artistMixed ? 'Various values' : 'Photographer name…'"
        />
      </label>
      <label class="ps-edit-row">
        <span class="ps-label">Copyright</span>
        <input
          class="ps-input"
          type="text"
          :value="copyright"
          @input="e => { emit('update:copyright', e.target.value); emit('change') }"
          :placeholder="copyrightMixed ? 'Various values' : '© Year Name…'"
        />
      </label>
    </div>

    <p class="ps-save-notice" v-if="showNotice">
      Changes are written directly to the file's EXIF data.
    </p>
  </div>
</template>

<script setup>
import ChevronIcon from './ChevronIcon.vue'

defineProps({
  description:    String,
  artist:         String,
  copyright:      String,
  descMixed:      { type: Boolean, default: false },
  artistMixed:    { type: Boolean, default: false },
  copyrightMixed: { type: Boolean, default: false },
  showNotice:     { type: Boolean, default: false },
  collapsible:    { type: Boolean, default: false },
  collapsed:      { type: Boolean, default: true },
})
const emit = defineEmits(['toggle', 'update:description', 'update:artist', 'update:copyright', 'change'])
</script>

<style scoped>
.ps-section-title {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 10px;
  font-weight: 700;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.6px;
  white-space: nowrap;
  background: none;
  border: none;
  padding: 0;
  text-align: left;
  line-height: 1;
  flex-shrink: 0;
}
button.ps-section-title {
  width: 100%;
  height: 36px;
  cursor: pointer;
}
button.ps-section-title:hover { color: var(--text-secondary); }

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

.ps-label {
  color: var(--text-muted);
  font-size: 10px;
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

.ps-save-notice {
  font-size: 10px;
  color: var(--text-muted);
  opacity: 0.7;
  margin-top: var(--space-2);
}
</style>
