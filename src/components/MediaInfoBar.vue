<template>
  <div :class="inline ? 'info-inline' : 'info-bar'">
    <span class="info-total">{{ total }} files</span>
    <span class="info-sep" />
    <span class="info-dot info-dot--images" />
    <span class="info-label">Images</span>
    <span class="info-count">{{ images }}</span>
    <div class="ext-list">
      <span v-for="(count, ext) in imageExts" :key="ext" class="ext-pill ext-pill--images">.{{ ext }} <em>{{ count }}</em></span>
    </div>
    <template v-if="showVideos">
      <span class="info-sep" />
      <span class="info-dot info-dot--videos" />
      <span class="info-label">Videos</span>
      <span class="info-count">{{ videos }}</span>
      <div class="ext-list">
        <span v-for="(count, ext) in videoExts" :key="ext" class="ext-pill ext-pill--videos">.{{ ext }} <em>{{ count }}</em></span>
      </div>
    </template>
  </div>
</template>

<script setup>
defineProps({
  total:      { type: Number, default: 0 },
  images:     { type: Number, default: 0 },
  videos:     { type: Number, default: 0 },
  imageExts:  { type: Object, default: () => ({}) },
  videoExts:  { type: Object, default: () => ({}) },
  showVideos: { type: Boolean, default: true },
  // inline = drop the full-width bar chrome and flow within an existing toolbar.
  inline:     { type: Boolean, default: false },
})
</script>

<style scoped>
.info-bar {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: 0 var(--space-4);
  height: 44px;
  border-bottom: 1px solid var(--border-color);
  background: var(--bg-secondary);
  flex-shrink: 0;
  flex-wrap: nowrap;
  overflow: hidden;
}
/* Inline variant: no bar chrome, flows inside an existing action/toolbar row. */
.info-inline {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  flex-wrap: nowrap;
  overflow: hidden;
  min-width: 0;
}
.info-total {
  font-size: var(--font-size-sm);
  font-weight: 600;
  color: var(--text-primary);
  white-space: nowrap;
}
.info-sep {
  width: 1px;
  height: 16px;
  background: var(--border-color);
  flex-shrink: 0;
}
.info-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}
.info-dot--images { background: var(--color-accent); }
.info-dot--videos { background: var(--color-success); }
.info-label {
  font-size: var(--font-size-sm);
  color: var(--text-secondary);
  white-space: nowrap;
}
.info-count {
  font-size: var(--font-size-sm);
  font-weight: 700;
  color: var(--text-primary);
  white-space: nowrap;
  margin-right: 2px;
}

/* Extension pills */
.ext-list {
  display: flex;
  flex-wrap: nowrap;
  gap: 4px;
}
.ext-pill {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 7px;
  border-radius: var(--border-radius-pill);
  font-size: 10px;
  font-weight: 500;
  letter-spacing: 0.3px;
  white-space: nowrap;
}
.ext-pill em {
  font-style: normal;
  font-weight: 700;
  opacity: 0.8;
}
.ext-pill--images {
  background: color-mix(in srgb, var(--color-accent) 15%, transparent);
  color: var(--color-accent);
}
.ext-pill--videos {
  background: color-mix(in srgb, var(--color-success) 15%, transparent);
  color: var(--color-success);
}
</style>
