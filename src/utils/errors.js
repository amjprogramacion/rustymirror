/**
 * Extracts a human-readable message from a Tauri command error.
 *
 * Tauri commands that return Result<T, AppError> reject with a structured
 * object like { type: "scan", message: "..." } or { type: "delete", path:
 * "/foo/bar.jpg", message: "..." }. Legacy commands still reject with a plain
 * string. This helper handles both cases so call sites stay uniform.
 *
 * @param {unknown} e - The caught error value from an invoke() rejection.
 * @returns {string} A human-readable error message.
 */
export function errorMessage(e) {
  if (e == null) return 'Unknown error'
  if (typeof e === 'string') return e
  if (typeof e === 'object' && typeof e.message === 'string') return e.message
  return String(e)
}
