import { ref, watch } from 'vue'

// ── Singleton: refs defined at module level so all callers share the same state ─
const maxHistory       = ref(parseInt(localStorage.getItem('rustymirror_max_history')       ?? '5',  10))
const thumbConcurrency = ref(parseInt(localStorage.getItem('rustymirror_thumb_concurrency') ?? '4',  10))
const crossDatePhash   = ref(localStorage.getItem('rustymirror_cross_date_phash') !== 'false')
const fastMode         = ref(localStorage.getItem('rustymirror_fast_mode')        === 'true')
const autoUpdate       = ref(localStorage.getItem('rustymirror_auto_update')      !== 'false')
const notifyOnUpdate   = ref(localStorage.getItem('rustymirror_notify_update')    !== 'false')
const sidebarWidth     = ref(parseInt(localStorage.getItem('rustymirror_sidebar_width') ?? '240', 10))

// ── Auto-persist on change ────────────────────────────────────────────────────
watch(maxHistory,       v => localStorage.setItem('rustymirror_max_history',       String(v)))
watch(thumbConcurrency, v => localStorage.setItem('rustymirror_thumb_concurrency', String(v)))
watch(crossDatePhash,   v => localStorage.setItem('rustymirror_cross_date_phash',  String(v)))
watch(fastMode,         v => localStorage.setItem('rustymirror_fast_mode',         String(v)))
watch(autoUpdate,       v => localStorage.setItem('rustymirror_auto_update',       String(v)))
watch(notifyOnUpdate,   v => localStorage.setItem('rustymirror_notify_update',     String(v)))
watch(sidebarWidth,     v => localStorage.setItem('rustymirror_sidebar_width',     String(v)))

export function useSettings() {
  return { maxHistory, thumbConcurrency, crossDatePhash, fastMode, autoUpdate, notifyOnUpdate, sidebarWidth }
}
