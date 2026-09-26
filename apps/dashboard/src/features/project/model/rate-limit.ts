export type ParsedRateLimit =
  | { ok: true; value: number | null }
  | { ok: false };

/**
 * A rate-limit field as the API reads it: empty means the project follows the
 * server's limit (`null`), otherwise a whole number of at least one that a
 * JavaScript number holds exactly.
 */
export function parseRateLimit(raw: string): ParsedRateLimit {
  const trimmed = raw.trim();
  if (trimmed === '') return { ok: true, value: null };
  if (!/^\d+$/.test(trimmed)) return { ok: false };
  const value = Number(trimmed);
  // Past MAX_SAFE_INTEGER the number rounds, and a different limit is saved.
  return value >= 1 && Number.isSafeInteger(value)
    ? { ok: true, value }
    : { ok: false };
}
