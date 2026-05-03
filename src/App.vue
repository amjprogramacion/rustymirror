<template>
  <div class="app-layout">
    <ModeRail />
    <Sidebar />
    <main class="content-area">
      <DuplicatesView v-if="activeMode === 'duplicates'" />
      <MetadataView v-else-if="activeMode === 'metadata'" />
      <OrganizerView v-else />
    </main>
  </div>
  <Lightbox />
  <UpdateToast />
</template>

<script setup>
import { onMounted, watch } from 'vue'
import ModeRail from './components/ModeRail.vue'
import Sidebar from './components/Sidebar.vue'
import DuplicatesView from './components/DuplicatesView.vue'
import MetadataView from './components/MetadataView.vue'
import OrganizerView from './components/OrganizerView.vue'
import Lightbox from './components/Lightbox.vue'
import UpdateToast from './components/UpdateToast.vue'
import { useUpdater } from './composables/useUpdater'
import { useMode } from './composables/useMode'
import { usePanelStore } from './store/panel'
import { useMetadataStore } from './store/metadata'

const { activeMode } = useMode()
const panel = usePanelStore()

// Each tool keeps its own independent panel state.
// When switching modes: stash the current panel, restore the one for the new mode.
const panelStash = {}
watch(activeMode, (newMode, oldMode) => {
  panelStash[oldMode] = panel.activePanel
  panel.activePanel = panelStash[newMode] ?? null
})

const { autoCheck, checkForUpdates } = useUpdater()
const metaStore = useMetadataStore()

onMounted(() => {
  if (autoCheck.value) checkForUpdates({ notify: true, silent: true })
  metaStore.loadDeviceAliases()
})
</script>

<style>
.app-layout {
  display: flex;
  height: 100vh;
  overflow: hidden;
}

.content-area {
  flex: 1;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
</style>
