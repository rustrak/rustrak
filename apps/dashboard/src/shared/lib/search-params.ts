/**
 * Reading the query string. Whatever `validateSearch` returns is the canonical
 * URL, so an absent parameter stays absent rather than becoming an explicit
 * default, and an unparseable one is treated as absent rather than as an error.
 */

/** A query parameter as a non-empty string, or absent. */
export function searchString(value: unknown): string | undefined {
  if (typeof value !== 'string') return undefined;
  return value.length > 0 ? value : undefined;
}

/**
 * A 1-based page number, or absent.
 *
 * Anything that is not a page — `0`, `-3`, `abc`, an array — reads as absent,
 * and every call site then applies its own `?? 1`. That extra step is what
 * keeps `/projects` spelled `/projects`: whatever this returns is the route's
 * canonical search, so answering `1` for a missing parameter would write
 * `?page=1` into an address that never had one.
 */
export function searchPage(value: unknown): number | undefined {
  const raw =
    typeof value === 'number' ? value : Number.parseInt(String(value), 10);
  return Number.isFinite(raw) && raw >= 1 ? Math.floor(raw) : undefined;
}

/**
 * A query parameter constrained to a known set.
 *
 * A value outside the set reads as absent rather than being passed on. The API
 * ignores what it cannot parse and answers with unfiltered data, which would
 * leave a filter in the URL that no control shows as selected — the URL and the
 * screen disagreeing is the failure this prevents.
 */
export function searchOneOf<T extends string>(
  value: unknown,
  allowed: readonly T[],
): T | undefined {
  const raw = searchString(value);
  return raw !== undefined && (allowed as readonly string[]).includes(raw)
    ? (raw as T)
    : undefined;
}

/**
 * A record id: a positive safe integer written as one (`3`, never `3abc` or
 * `3.5`). TanStack hands a numeric query value over as a number on a direct
 * page load, so both spellings are read.
 */
export function searchId(value: unknown): number | undefined {
  const id = typeof value === 'number' ? String(value) : value;
  if (typeof id !== 'string' || !/^[1-9]\d*$/.test(id)) return undefined;
  const parsed = Number(id);
  return Number.isSafeInteger(parsed) ? parsed : undefined;
}

/**
 * Some record ids, comma-separated (`3,7`), each read as [`searchId`] reads
 * one. Absent when none of them is an id.
 */
export function searchIds(value: unknown): number[] | undefined {
  const raw = typeof value === 'number' ? String(value) : value;
  if (typeof raw !== 'string') return undefined;
  const ids = [
    ...new Set(raw.split(',').flatMap((part) => searchId(part.trim()) ?? [])),
  ];
  return ids.length > 0 ? ids : undefined;
}
