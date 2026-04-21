<template>
  <CollapsibleSection :collapsible="collapsible" :collapsed="collapsed" @toggle="emit('toggle')">
    <template #title>Details</template>

    <div class="ps-edit-rows">
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
  </CollapsibleSection>
</template>

<script setup>
import CollapsibleSection from './CollapsibleSection.vue'
import '../styles/panel-sections.css'

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
