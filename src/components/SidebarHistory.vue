<template>
  <template v-if="entries.length > 0">
    <SidebarDivider />
    <section class="sidebar-section history-section">
      <p class="section-label">Recent scans</p>
      <div class="history-entries-scroll">
        <div
          v-for="entry in entries"
          :key="entry.id"
          class="history-entry"
          :class="{
            disabled,
            active: entry.id === activeId,
            'history-entry--missing': folderStatus[entry.id] === 'missing',
            'history-entry--partial': folderStatus[entry.id] === 'partial',
          }"
          @click="emit('select', entry)"
          :title="entry.folders.join('\n')"
        >
          <div class="history-date">
            {{ formatLocalDate(entry.date) }}<span v-if="formatDuration(entry.durationMs)" class="history-duration">&nbsp;({{ formatDuration(entry.durationMs) }})</span>
          </div>
          <div class="history-folders">
            <span
              v-for="f in entry.folders" :key="f"
              class="history-folder"
              :class="{ 'history-folder--missing': missingFolders[entry.id]?.includes(f) }"
              :title="f"
            >{{ shortPath(f) }}</span>
          </div>
          <div class="history-footer">
            <slot name="stats" :entry="entry" />
          </div>
          <span
            v-if="folderStatus[entry.id] === 'missing' || folderStatus[entry.id] === 'partial'"
            class="folder-alert"
            :title="folderStatus[entry.id] === 'missing' ? 'Folder no longer exists' : 'Some folders no longer exist'"
          >
            <IconWarning />
          </span>
          <button
            class="history-remove"
            title="Remove from history"
            @click.stop="emit('remove', entry.id)"
          >
            <IconClose />
          </button>
        </div>
      </div>
    </section>
  </template>
</template>

<script setup>
import { shortPath, formatLocalDate, formatDuration } from '../utils/formatters'
import IconWarning from './IconWarning.vue'
import IconClose from './IconClose.vue'
import SidebarDivider from './SidebarDivider.vue'

defineProps({
  entries:        { type: Array,  required: true },
  folderStatus:   { type: Object, default: () => ({}) },
  missingFolders: { type: Object, default: () => ({}) },
  activeId:       { default: null },
  disabled:       { type: Boolean, default: false },
})
const emit = defineEmits(['select', 'remove'])
</script>
