<template>
  <div class="app-layout">
    <ModeRail />
    <Sidebar />
    <main class="content-area">
      <ResultsArea v-if="activeMode === 'duplicates'" />
      <MetadataManager v-else />
    </main>
  </div>
  <Lightbox />
  <UpdateToast />
</template>

<script setup>
import { onMounted, watch } from 'vue'
import ModeRail from './components/ModeRail.vue'
import Sidebar from './components/Sidebar.vue'
import ResultsArea from './components/ResultsArea.vue'
import MetadataManager from './components/MetadataManager.vue'
import Lightbox from './components/Lightbox.vue'
import UpdateToast from './components/UpdateToast.vue'
import { useUpdater } from './composables/useUpdater'
import { useMode } from './composables/useMode'
import { useScanStore } from './store/scan'

const { activeMode } = useMode()
const scanStore = useScanStore()

// Each tool keeps its own independent metadata panel state.
// When switching modes: stash the current panel, restore the one for the new mode.
const panelStash = {}
watch(activeMode, (newMode, oldMode) => {
  panelStash[oldMode] = scanStore.metadataPanel
  scanStore.$patch({ metadataPanel: panelStash[newMode] ?? null })
})

const { autoCheck, checkForUpdates } = useUpdater()

onMounted(() => {
  if (autoCheck.value) checkForUpdates({ notify: true, silent: true })
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
