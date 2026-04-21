// ── Sidebar ───────────────────────────────────────────────────────────────────
export const SIDEBAR_MIN_WIDTH = 200
export const SIDEBAR_MAX_WIDTH = 420

// ── Metadata panel ────────────────────────────────────────────────────────────
export const MP_MIN_WIDTH        = 450
export const MP_MIN_THUMB_HEIGHT = 200

// ── Delete confirmation dialog ────────────────────────────────────────────────
export const DELETE_MAX_PREVIEW = 10

// ── Scan history ─────────────────────────────────────────────────────────────
export const HISTORY_MAX_ENTRIES = 5

// ── Geocoding ─────────────────────────────────────────────────────────────────
export const GEOCODE_DEBOUNCE_MS = 600

// ── Panel sections ────────────────────────────────────────────────────────────
// Sentinel used by batch-edit aggregations to flag fields whose values differ
// across the selected files. Panel sections render "Various values" when they
// receive this sentinel.
export const MIXED_VALUE = '__mixed__'
