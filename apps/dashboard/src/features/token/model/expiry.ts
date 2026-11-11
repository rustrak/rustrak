/**
 * The date field as the API reads it: empty is a token that never expires
 * (`null`), otherwise the end of the chosen day in the reader's zone, which
 * must still be ahead: a token that expired on creation is no token at all.
 */
export function tokenExpiry(
  day: string,
  now: Date = new Date(),
): { ok: true; value: string | null } | { ok: false } {
  if (!day) return { ok: true, value: null };
  const end = new Date(`${day}T23:59:59`);
  return end > now ? { ok: true, value: end.toISOString() } : { ok: false };
}
