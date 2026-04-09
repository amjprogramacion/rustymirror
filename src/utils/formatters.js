// ── File path helpers ─────────────────────────────────────────────────────────

export function fileExt(p) {
  return p?.split('.').pop()?.toLowerCase() ?? ''
}

export function fileName(p) {
  return p?.split(/[/\\]/).pop() ?? ''
}

export function folderPath(p) {
  const parts = p?.split(/[/\\]/) ?? []
  return parts.slice(0, -1).join('/')
}

// Shows: drive + "…" + last 3 folders  e.g.  N:\…\FOTOS\GALERÍA\2019
export function shortPath(p) {
  const sep = p.includes('\\') ? '\\' : '/'
  const parts = p.split(/[/\\]/).filter(Boolean)
  if (parts.length <= 4) return p
  const isUnc = p.startsWith('\\\\') || p.startsWith('//')
  const drive = isUnc
    ? sep + sep + parts[0] + sep
    : p.match(/^[a-zA-Z]:/) ? parts[0] + sep
    : parts[0] + sep
  return drive + '…' + sep + parts.slice(-3).join(sep)
}

// ── Size formatting ───────────────────────────────────────────────────────────

export function formatSize(b) {
  if (!b && b !== 0) return ''
  if (b === 0)       return '0 B'
  if (b < 1024)      return `${b} B`
  if (b < 1048576)   return `${(b / 1024).toFixed(1)} KB`
  return `${(b / 1048576).toFixed(1)} MB`
}

// ── Date formatting ───────────────────────────────────────────────────────────

// ISO 8601 → "DD/MM/YYYY HH:MM:SS"
// Always parsed directly from the string — never via Date — because dates
// in this app (EXIF and filesystem alike) carry the local wall-clock time
// regardless of any Z suffix, so timezone conversion must be skipped.
export function formatDate(iso, fallback = '') {
  if (!iso || iso.startsWith('1970')) return fallback
  const m = iso.match(/^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})/)
  if (!m) return fallback
  return `${m[3]}/${m[2]}/${m[1]} ${m[4]}:${m[5]}:${m[6]}`
}

// ISO 8601 → "DD/MM/YYYY HH:MM" (local time) — used for scan history timestamps
export function formatLocalDate(iso) {
  const d = new Date(iso)
  const pad = n => String(n).padStart(2, '0')
  return `${pad(d.getDate())}/${pad(d.getMonth() + 1)}/${d.getFullYear()} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

// Scan duration: ms → human string ("3s", "1m 20s", "2h 5m")
export function formatDuration(ms) {
  if (ms === -1)         return 'cancelled'
  if (!ms || ms < 500)   return null
  const s = Math.round(ms / 1000)
  if (s < 60) return `${s}s`
  const m = Math.floor(s / 60)
  const r = s % 60
  if (m < 60) return r > 0 ? `${m}m ${r}s` : `${m}m`
  const h = Math.floor(m / 60)
  const rm = m % 60
  return rm > 0 ? `${h}h ${rm}m` : `${h}h`
}

// ── GPS formatting ────────────────────────────────────────────────────────────

export function formatGps(lat, lon) {
  const fmt = (v, pos, neg) => `${Math.abs(v).toFixed(5)}° ${v >= 0 ? pos : neg}`
  return `${fmt(lat, 'N', 'S')}, ${fmt(lon, 'E', 'W')}`
}

// ── Datetime-local <input> helpers ────────────────────────────────────────────

// "2023-06-15T14:30:00Z" → "2023-06-15T14:30:00"
export function isoToDatetimeLocal(iso) {
  if (!iso) return ''
  const s = iso.replace('Z', '')
  return s.length >= 19 ? s.slice(0, 19) : s.slice(0, 16) + ':00'
}

// "2023-06-15T14:30" → "2023-06-15T14:30:00"
export function datetimeLocalToIso(v) {
  if (!v) return null
  return v.length === 16 ? `${v}:00` : v
}

// ── Duplicate kind label ──────────────────────────────────────────────────────

export function kindLabel(kind) {
  if (kind === 'exact')    return 'Exact duplicate'
  if (kind === 'similar')  return 'Similar'
  if (kind === 'sameDate') return 'Same date'
  return kind
}
