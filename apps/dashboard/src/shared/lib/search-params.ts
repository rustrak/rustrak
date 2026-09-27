/**
 * Reading the query string, once, the way every screen used to read it.
 *
 * Under Next a page took `searchParams` as `Promise<{page?: string}>` and did
 * its own parsing: `parseInt(page ?? '1', 10) || 1`. The router hands the query
 * to `validateSearch` instead, and whatever that returns becomes the typed
 * search *and* the canonical URL — so a validator that invents a value writes
 * it into the address bar, and one that drops a value removes it.
 *
 * These two keep to what the pages already did, so an address that worked
 * before still works and still looks the same:
 *
 * - an absent parameter stays absent, rather than becoming an explicit default
 * - an unparseable one is treated as absent, rather than as an error
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
 * Where to go after signing in: a path on this origin, or absent.
 *
 * Only an absolute path counts. `//host` and `/\host` are read by browsers as
 * another origin, and a scheme is one by definition, so all of them are
 * dropped rather than followed.
 */
export function searchRedirect(value: unknown): string | undefined {
  const raw = searchString(value);
  if (raw === undefined || !raw.startsWith('/')) return undefined;
  if (raw.startsWith('//') || raw.startsWith('/\\')) return undefined;
  if (/^\/login(?:[/?#]|$)/.test(raw)) return undefined;
  return raw;
}
