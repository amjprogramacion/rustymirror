import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

// Shared singleton — both Sidebar and SettingsModal read from the same refs
const cacheSize      = ref(0)
const thumbCacheSize = ref(0)

async function loadCacheSizes() {
  try { cacheSize.value      = await invoke('get_cache_size') }       catch (e) { console.warn('get_cache_size failed:', e) }
  try { thumbCacheSize.value = await invoke('get_thumb_cache_size') } catch (e) { console.warn('get_thumb_cache_size failed:', e) }
}

async function clearCache() {
  try { await invoke('clear_cache');       cacheSize.value = 0 }      catch (e) { console.warn(e) }
}

async function clearThumbCache() {
  try { await invoke('clear_thumb_cache'); thumbCacheSize.value = 0 } catch (e) { console.warn(e) }
}

export function useCacheSize() {
  return { cacheSize, thumbCacheSize, loadCacheSizes, clearCache, clearThumbCache }
}
