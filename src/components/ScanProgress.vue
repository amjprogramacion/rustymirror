<template>
  <div class="scan-overlay">
    <div class="scan-card">
      <div class="spinner" v-if="fingerprinting || isScanning || isAnalyzing" />

      <!-- Fingerprinting: folder analysis before scan -->
      <template v-if="fingerprinting">
        <p class="scan-title">{{ title || 'Analyzing selected folders…' }}</p>
        <div class="bar-track"><div class="bar-indeterminate" /></div>
      </template>

      <!-- Determinate scan phase: file count + progress bar + ETA -->
      <template v-else-if="isScanning">
        <p class="scan-title">{{ scanLabel || title || 'Scanning…' }}</p>
        <p class="scan-count">
          <span class="count-current">{{ progress.scanned.toLocaleString() }}</span>
          <span class="count-sep"> / </span>
          <span class="count-total">{{ progress.total.toLocaleString() }}</span>
          <span class="count-label"> images</span>
        </p>
        <div class="bar-track">
          <div class="bar-fill" :style="{ width: progressPercent + '%' }" />
        </div>
        <div class="scan-meta">
          <span class="pct">{{ progressPercent }}%</span>
          <span class="eta">{{ etaLabel || '\xa0' }}</span>
        </div>
      </template>

      <!-- Determinate analyze phase: phase progress bar -->
      <template v-else-if="isAnalyzing">
        <p class="scan-title">{{ scanLabel || analyzeProgress.phase || title || 'Analyzing…' }}</p>
        <p class="scan-count">
          <span class="count-current">{{ analyzeProgress.analyzed.toLocaleString() }}</span>
          <span class="count-sep"> / </span>
          <span class="count-total">{{ analyzeProgress.total.toLocaleString() }}</span>
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

      <!-- Indeterminate fallback: simple title + optional subtitle -->
      <template v-else>
        <p class="scan-title">{{ scanLabel || analyzeProgress.phase || title || 'Scanning…' }}</p>
        <p v-if="subtitle" class="scan-subtitle">{{ subtitle }}</p>
        <div class="bar-track"><div class="bar-indeterminate" /></div>
      </template>

    </div>
  </div>
</template>

<script setup>
import { computed } from 'vue'

const props = defineProps({
  // Generic title shown in all indeterminate phases
  title:           { type: String,  default: null },
  // Optional secondary line (e.g. "1 234 images found" during geocoding)
  subtitle:        { type: String,  default: null },
  // Duplicates-specific: pre-scan folder fingerprinting phase
  fingerprinting:  { type: Boolean, default: false },
  // Duplicates-specific: label override set by the store mid-scan
  scanLabel:       { type: String,  default: null },
  // Duplicates-specific: file scan progress { scanned, total }
  progress:        { type: Object,  default: () => ({ scanned: 0, total: 0 }) },
  progressPercent: { type: Number,  default: 0 },
  // Duplicates-specific: analysis phase progress { analyzed, total, phase }
  analyzeProgress: { type: Object,  default: () => ({ analyzed: 0, total: 0, phase: '' }) },
  etaSeconds:      { type: Number,  default: null },
})

const isScanning = computed(() =>
  (props.progress?.total ?? 0) > 0 &&
  props.progress.scanned < props.progress.total
)

const isAnalyzing = computed(() =>
  (props.analyzeProgress?.total ?? 0) > 1
)

const analyzePct = computed(() => {
  const { analyzed, total } = props.analyzeProgress ?? {}
  if (!total) return 0
  return Math.round((analyzed / total) * 100)
})

const etaLabel = computed(() => {
  const s = props.etaSeconds
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

.scan-subtitle {
  font-size: var(--font-size-sm);
  color: var(--text-muted);
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

.bar-analyze { background: var(--color-success); }

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
