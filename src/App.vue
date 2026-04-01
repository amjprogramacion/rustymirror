<template>
  <div class="app-layout">
    <Sidebar />
    <main class="content-area">
      <ResultsArea />
    </main>
  </div>
  <Lightbox />
</template>

<script setup>
import { onMounted } from 'vue'
import Sidebar from './components/Sidebar.vue'
import ResultsArea from './components/ResultsArea.vue'
import Lightbox from './components/Lightbox.vue'
import { useUpdater } from './composables/useUpdater'

const { autoCheck, checkForUpdates } = useUpdater()

onMounted(() => {
  if (autoCheck.value) checkForUpdates()
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
