export type ParsedRateLimit =
  | { ok: true; value: number | null }
  | { ok: false; tooHigh?: true };

/**
 * A rate-limit field as the API reads it: empty means the project follows the
 * server's limit (`null`), otherwise a whole number of at least one that a
 * JavaScript number holds exactly, and no higher than the server's own limit
 * (`max`), which a project can only lower.
 */
export function parseRateLimit(raw: string, max?: number): ParsedRateLimit {
  const trimmed = raw.trim();
  if (trimmed === '') return { ok: true, value: null };
  if (!/^\d+$/.test(trimmed)) return { ok: false };
  const value = Number(trimmed);
  // Past MAX_SAFE_INTEGER the number rounds, and a different limit is saved.
  if (value < 1 || !Number.isSafeInteger(value)) return { ok: false };
  return max !== undefined && value > max
    ? { ok: false, tooHigh: true }
    : { ok: true, value };
}
