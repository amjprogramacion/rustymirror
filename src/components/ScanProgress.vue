<template>
  <div class="scan-overlay">
    <div class="scan-card">
      <div class="spinner" />

      <!-- Fingerprinting phase: double WalkDir pass -->
      <template v-if="store.fingerprinting">
        <p class="scan-title">Analyzing selected folders…</p>
        <div class="bar-track">
          <div class="bar-indeterminate" />
        </div>
      </template>

      <!-- Phase 1: scanning files -->
      <template v-else-if="isScanning">
        <p class="scan-title">{{ store.scanLabel || 'Scanning for duplicates…' }}</p>
        <p class="scan-count">
          <span class="count-current">{{ store.progress.scanned.toLocaleString() }}</span>
          <span class="count-sep"> / </span>
          <span class="count-total">{{ store.progress.total.toLocaleString() }}</span>
          <span class="count-label"> images</span>
        </p>
        <div class="bar-track">
          <div class="bar-fill" :style="{ width: store.progressPercent + '%' }" />
        </div>
        <div class="scan-meta">
          <span class="pct">{{ store.progressPercent }}%</span>
          <span class="eta">{{ etaLabel || '\xa0' }}</span>
        </div>
      </template>

      <!-- Phase 2-4: analyzing -->
      <template v-else>
        <p class="scan-title">{{ store.scanLabel || analyzeTitle }}</p>
        <template v-if="store.analyzeProgress.total > 1">
          <p class="scan-count">
            <span class="count-current">{{ store.analyzeProgress.analyzed.toLocaleString() }}</span>
            <span class="count-sep"> / </span>
            <span class="count-total">{{ store.analyzeProgress.total.toLocaleString() }}</span>
            <span class="count-label"> files</span>
          </p>
          <div class="bar-track">
            <div class="bar-fill bar-analyze" :style="{ width: analyzePct + '%' }" />
          </div>
          <div class="scan-meta">
            <span class="pct">{{ analyzePct }}%</span>
            <span class="eta">&nbsp;</span>
          </div>
        </template>
        <template v-else>
          <!-- Indeterminate — no count available yet -->
          <div class="bar-track">
            <div class="bar-indeterminate" />
          </div>
        </template>
      </template>

    </div>
  </div>
</template>

<script setup>
import { computed } from 'vue'
import { useScanStore } from '../store/scan'

const store = useScanStore()

const isScanning = computed(() =>
  store.progress.total > 0 &&
  store.progress.scanned < store.progress.total
)

const analyzePct = computed(() => {
  const { analyzed, total } = store.analyzeProgress
  if (!total) return 0
  return Math.round((analyzed / total) * 100)
})

const analyzeTitle = computed(() =>
  store.analyzeProgress.phase || 'Analyzing…'
)

const etaLabel = computed(() => {
  const s = store.etaSeconds
  if (!s) return ''
  if (s < 60) return `~${s}s remaining`
  return `~${Math.ceil(s / 60)}m remaining`
})
</script>

<style scoped>
.scan-overlay {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--bg-primary);
}

.scan-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-6);
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-lg);
  width: 360px;
}

@keyframes spin { to { transform: rotate(360deg); } }
.spinner {
  width: 36px;
  height: 36px;
  border: 3px solid var(--border-color);
  border-top-color: var(--color-accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

.scan-title {
  font-size: var(--font-size-md);
  font-weight: 500;
  color: var(--text-primary);
  text-align: center;
}

.scan-count { font-size: var(--font-size-lg); color: var(--text-secondary); }
.count-current { font-weight: 600; color: var(--color-accent); font-size: 22px; }
.count-sep, .count-total { color: var(--text-secondary); font-size: 18px; }
.count-label { font-size: var(--font-size-sm); color: var(--text-muted); }

.bar-track {
  width: 100%;
  height: 6px;
  background: var(--bg-secondary);
  border-radius: var(--border-radius-pill);
  overflow: hidden;
}

.bar-fill {
  height: 100%;
  background: var(--color-accent);
  border-radius: var(--border-radius-pill);
  transition: width 250ms linear;
}

.bar-analyze {
  background: var(--color-success);
}

@keyframes indeterminate {
  0%   { transform: translateX(-100%); }
  100% { transform: translateX(400%); }
}
.bar-indeterminate {
  height: 100%;
  width: 25%;
  background: var(--color-success);
  border-radius: var(--border-radius-pill);
  animation: indeterminate 1.4s ease-in-out infinite;
}

.scan-meta {
  width: 100%;
  display: flex;
  justify-content: space-between;
  font-size: var(--font-size-xs);
}

.pct  { color: var(--text-secondary); font-weight: 500; }
.eta  { color: var(--text-muted); }
</style>
