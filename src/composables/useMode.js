import { ref, watch } from 'vue'

const VALID_MODES = ['duplicates', 'metadata']
const stored      = localStorage.getItem('rustymirror_active_mode')
const activeMode  = ref(VALID_MODES.includes(stored) ? stored : 'duplicates')

watch(activeMode, v => localStorage.setItem('rustymirror_active_mode', v))

export function useMode() {
  return { activeMode }
}
