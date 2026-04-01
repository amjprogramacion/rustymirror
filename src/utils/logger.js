import { invoke } from '@tauri-apps/api/core'

const send = (level, ...args) => {
  const message = args.map(a =>
    typeof a === 'object' ? JSON.stringify(a, null, 2) : String(a)
  ).join(' ')
  // Also keep browser console working
  console[level === 'error' ? 'error' : level === 'warn' ? 'warn' : 'log']('[RustyMirror]', message)
  invoke('log_message', { level, message }).catch(() => {})
}

export const logger = {
  info:  (...args) => send('info',  ...args),
  warn:  (...args) => send('warn',  ...args),
  error: (...args) => send('error', ...args),
}
