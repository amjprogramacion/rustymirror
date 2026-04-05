<template>
  <div class="app-layout">
    <Sidebar />
    <main class="content-area">
      <ResultsArea />
    </main>
  </div>
  <Lightbox />
  <MetadataPanel />

  <!-- Update toast -->
  <Transition name="toast-slide">
    <div v-if="showNotification" class="update-toast">
      <div class="update-toast-body">
        <span class="update-toast-icon">⚡</span>
        <div class="update-toast-text">
          <span class="update-toast-title">
            <template v-if="updateStatus === 'ready'">Update installed</template>
            <template v-else>Update available</template>
          </span>
          <span class="update-toast-version">
            <template v-if="updateStatus === 'downloading'">
              Downloading{{ downloadProgress >= 0 ? ` ${downloadProgress}%` : '…' }}
            </template>
            <template v-else-if="updateStatus === 'ready'">Restart to apply {{ latestVersion }}</template>
            <template v-else>{{ latestVersion }} is ready to install</template>
          </span>
        </div>
      </div>
      <div v-if="updateStatus === 'downloading'" class="update-toast-progress">
        <div class="update-toast-progress-bar" :style="{ width: downloadProgress >= 0 ? `${downloadProgress}%` : '100%' }" />
      </div>
      <div v-else class="update-toast-actions">
        <template v-if="updateStatus === 'ready'">
          <button class="toast-btn toast-btn-primary" @click="restartApp">Restart now</button>
          <button class="toast-btn toast-btn-dismiss" @click="showNotification = false">Later</button>
        </template>
        <template v-else>
          <button class="toast-btn toast-btn-primary" @click="installUpdate">Install</button>
          <button class="toast-btn toast-btn-dismiss" @click="showNotification = false">Dismiss</button>
        </template>
      </div>
    </div>
  </Transition>
</template>

<script setup>
import { onMounted } from 'vue'
import Sidebar from './components/Sidebar.vue'
import ResultsArea from './components/ResultsArea.vue'
import Lightbox from './components/Lightbox.vue'
import MetadataPanel from './components/MetadataPanel.vue'
import { useUpdater } from './composables/useUpdater'

const { autoCheck, showNotification, latestVersion, status: updateStatus, downloadProgress, checkForUpdates, installUpdate, restartApp } = useUpdater()

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

/* ── Update toast ── */
.update-toast {
  position: fixed;
  bottom: 20px;
  right: 20px;
  z-index: 500;
  background: var(--bg-secondary);
  border: 1px solid #f5c542;
  border-radius: var(--border-radius-lg);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.45);
  padding: 14px 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-width: 260px;
  max-width: 300px;
}

.update-toast-body {
  display: flex;
  align-items: center;
  gap: 10px;
}

.update-toast-icon {
  font-size: 20px;
  line-height: 1;
  flex-shrink: 0;
}

.update-toast-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.update-toast-title {
  font-size: var(--font-size-sm);
  font-weight: 600;
  color: var(--text-primary);
}

.update-toast-version {
  font-size: var(--font-size-xs);
  color: var(--text-muted);
}

.update-toast-progress {
  height: 4px;
  background: var(--bg-card);
  border-radius: 2px;
  overflow: hidden;
}

.update-toast-progress-bar {
  height: 100%;
  background: #f5c542;
  border-radius: 2px;
  transition: width 0.3s ease;
}

.update-toast-actions {
  display: flex;
  gap: 8px;
}

.toast-btn {
  flex: 1;
  padding: 5px 10px;
  border-radius: var(--border-radius-sm);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
}

.toast-btn-primary {
  background: #f5c542;
  color: #1a1200;
  border: none;
}
.toast-btn-primary:hover { background: #f0b800; }

.toast-btn-dismiss {
  background: transparent;
  color: var(--text-muted);
  border: 1px solid var(--border-color);
}
.toast-btn-dismiss:hover {
  background: var(--bg-card);
  color: var(--text-secondary);
}

/* ── Toast transition ── */
.toast-slide-enter-active,
.toast-slide-leave-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
}
.toast-slide-enter-from,
.toast-slide-leave-to {
  opacity: 0;
  transform: translateY(12px);
}
</style>
