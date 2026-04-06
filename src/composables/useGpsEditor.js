import { ref, computed, watch } from 'vue'
import { GEOCODE_DEBOUNCE_MS } from '../constants'

export function useGpsEditor(meta, onDirty) {
  const gpsLatitudeRaw   = ref('')
  const gpsLongitudeRaw  = ref('')
  const gpsLatError      = ref(null)
  const gpsLonError      = ref(null)
  const gpsCombinedRaw   = ref('')
  const gpsCombinedError = ref(null)

  const locationName    = ref(null)
  const locationLoading = ref(false)

  // ── Parsers ────────────────────────────────────────────────────────────────
  function parseGpsInput(raw) {
    if (!raw || !raw.trim()) return null
    const s = raw.trim()

    if (/^-?\d+(\.\d+)?$/.test(s)) return parseFloat(s)

    const dms = s.match(
      /^(\d+(?:\.\d+)?)\s*[°d]\s*(?:(\d+(?:\.\d+)?)\s*['′]\s*(?:(\d+(?:\.\d+)?)\s*["″]\s*)?)?([NSEWnsew])?$/
    )
    if (dms) {
      const deg = parseFloat(dms[1])
      const min = dms[2] ? parseFloat(dms[2]) : 0
      const sec = dms[3] ? parseFloat(dms[3]) : 0
      const dir = (dms[4] || '').toUpperCase()
      let decimal = deg + min / 60 + sec / 3600
      if (dir === 'S' || dir === 'W') decimal = -decimal
      return decimal
    }

    return null
  }

  function parseCombinedGps(raw) {
    if (!raw || !raw.trim()) return null
    const pattern = /(\d+(?:\.\d+)?)\s*[°d]\s*(?:(\d+(?:\.\d+)?)\s*['′]\s*(?:(\d+(?:\.\d+)?)\s*["″]\s*)?)?([NSEWnsew])/g
    const matches = [...raw.matchAll(pattern)]
    if (matches.length < 2) return null

    let lat = null, lon = null
    for (const m of matches) {
      const deg = parseFloat(m[1])
      const min = m[2] ? parseFloat(m[2]) : 0
      const sec = m[3] ? parseFloat(m[3]) : 0
      const dir = m[4].toUpperCase()
      let dec = deg + min / 60 + sec / 3600
      if (dir === 'S' || dir === 'W') dec = -dec
      if ('NS'.includes(dir)) lat = dec
      else lon = dec
    }

    return (lat !== null && lon !== null) ? { lat, lon } : null
  }

  // ── Computed ───────────────────────────────────────────────────────────────
  const showCombinedInput = computed(() =>
    meta.value?.gpsLatitude == null &&
    !gpsLatitudeRaw.value &&
    !gpsLongitudeRaw.value
  )

  const parsedLat = computed(() => parseGpsInput(gpsLatitudeRaw.value))
  const parsedLon = computed(() => parseGpsInput(gpsLongitudeRaw.value))

  const previewLat    = computed(() => parsedLat.value ?? meta.value?.gpsLatitude ?? null)
  const previewLon    = computed(() => parsedLon.value ?? meta.value?.gpsLongitude ?? null)
  const hasGpsPreview = computed(() => previewLat.value != null && previewLon.value != null)

  // ── Handlers ───────────────────────────────────────────────────────────────
  function onCombinedInput() {
    gpsCombinedError.value = null
    const result = parseCombinedGps(gpsCombinedRaw.value)
    if (result) {
      gpsLatitudeRaw.value  = result.lat.toFixed(6)
      gpsLongitudeRaw.value = result.lon.toFixed(6)
      gpsCombinedRaw.value  = ''
      onDirty()
    }
  }

  function onGpsInput(field) {
    if (field === 'lat') gpsLatError.value = null
    else gpsLonError.value = null
    onDirty()
  }

  function normalizeGpsInput(field) {
    const rawRef = field === 'lat' ? gpsLatitudeRaw  : gpsLongitudeRaw
    const errRef = field === 'lat' ? gpsLatError     : gpsLonError
    if (!rawRef.value || !rawRef.value.trim()) return
    const val = parseGpsInput(rawRef.value)
    if (val === null) {
      errRef.value = `Invalid ${field === 'lat' ? 'latitude' : 'longitude'}`
      return
    }
    errRef.value  = null
    rawRef.value  = val.toFixed(6)
  }

  // ── Reset (called from MetadataPanel when metadata loads) ──────────────────
  function resetGps(m) {
    gpsLatitudeRaw.value   = m?.gpsLatitude  != null ? m.gpsLatitude.toFixed(6)  : ''
    gpsLongitudeRaw.value  = m?.gpsLongitude != null ? m.gpsLongitude.toFixed(6) : ''
    gpsLatError.value      = null
    gpsLonError.value      = null
    gpsCombinedRaw.value   = ''
    gpsCombinedError.value = null
  }

  // ── Validation (called before save) ───────────────────────────────────────
  // Returns { ok: true, lat, lon } or sets errors and returns { ok: false }
  function validateGps() {
    const rawLat = gpsLatitudeRaw.value.trim()
    const rawLon = gpsLongitudeRaw.value.trim()
    const hasRaw = rawLat !== '' || rawLon !== ''

    if (!hasRaw) return { ok: true, lat: null, lon: null }

    const lat = parseGpsInput(rawLat)
    const lon = parseGpsInput(rawLon)
    if (rawLat && lat === null) { gpsLatError.value = 'Invalid latitude';               return { ok: false } }
    if (rawLon && lon === null) { gpsLonError.value = 'Invalid longitude';              return { ok: false } }
    if (rawLat && (lat < -90  || lat > 90))  { gpsLatError.value = 'Must be between -90 and 90';   return { ok: false } }
    if (rawLon && (lon < -180 || lon > 180)) { gpsLonError.value = 'Must be between -180 and 180'; return { ok: false } }

    const hasValid = lat !== null && lon !== null
    return { ok: true, lat: hasValid ? lat : null, lon: hasValid ? lon : null }
  }

  // ── Reverse geocoding ──────────────────────────────────────────────────────
  let geocodeTimer = null
  watch(
    () => [previewLat.value, previewLon.value],
    ([lat, lon]) => {
      clearTimeout(geocodeTimer)
      if (lat == null || lon == null) { locationName.value = null; return }
      locationLoading.value = true
      geocodeTimer = setTimeout(async () => {
        try {
          const res = await fetch(
            `https://nominatim.openstreetmap.org/reverse?lat=${lat}&lon=${lon}&format=json`,
            { headers: { 'User-Agent': 'RustyMirror/1.0 (desktop app)' } }
          )
          const data = await res.json()
          const addr = data.address ?? {}
          const city = addr.city ?? addr.town ?? addr.village ?? addr.municipality ?? addr.county ?? null
          locationName.value = [city, addr.country].filter(Boolean).join(', ') || null
        } catch {
          locationName.value = null
        } finally {
          locationLoading.value = false
        }
      }, GEOCODE_DEBOUNCE_MS)
    },
    { immediate: true }
  )

  return {
    gpsLatitudeRaw,
    gpsLongitudeRaw,
    gpsLatError,
    gpsLonError,
    gpsCombinedRaw,
    gpsCombinedError,
    locationName,
    locationLoading,
    showCombinedInput,
    previewLat,
    previewLon,
    hasGpsPreview,
    onCombinedInput,
    onGpsInput,
    normalizeGpsInput,
    resetGps,
    validateGps,
  }
}
