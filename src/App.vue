<template>
  <div class="app-layout">
    <Sidebar />
    <main class="content-area">
      <ResultsArea />
    </main>
  </div>
  <Lightbox />
  <MetadataPanel />
  <UpdateToast />
</template>

<script setup>
import { onMounted } from 'vue'
import Sidebar from './components/Sidebar.vue'
import ResultsArea from './components/ResultsArea.vue'
import Lightbox from './components/Lightbox.vue'
import MetadataPanel from './components/MetadataPanel.vue'
import UpdateToast from './components/UpdateToast.vue'
import { useUpdater } from './composables/useUpdater'

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
