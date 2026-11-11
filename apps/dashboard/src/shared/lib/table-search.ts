import {
  type DataTableQuery,
  type FilterVariants,
  parseTableQuery,
  serializeTableQuery,
} from '@rustrak/ui';

/** A table's search keys, exactly as `@rustrak/ui`'s codecs write them. */
export interface TableSearch {
  q?: string;
  sort?: string;
  // Numbers, not strings: the router quotes a numeric-looking string
  // (`?page=%222%22`) so it reads back as a string.
  page?: number;
  per?: number;
}

type Sort = DataTableQuery['sorting'][number];

const KEYS = ['q', 'sort', 'page', 'per'] as const;

/** The largest `per_page` any list endpoint serves (the server's pagination cap). */
const MAX_PAGE_SIZE = 100;

/**
 * The router hands numbers for `?page=3`, so anything not a non-empty string
 * is read through `String` first, and an empty one as absent.
 */
const raw = (value: unknown) =>
  value == null || value === '' ? undefined : String(value);

const num = (value: string | null) => (value ? Number(value) : undefined);

export interface TableCodec {
  variants: FilterVariants;
  pageSize: number;
  /**
   * What the page's API can read of a parsed query: the sorts it knows, one
   * value where it takes one. Applied on every
   * read, so whatever it drops never reaches the API or the address.
   */
  normalise?: (query: DataTableQuery) => DataTableQuery;
  /** The sort shown when the address names none; never written back to it. */
  defaultSort?: Sort;
}

/**
 * One table's search in and out of the address: `read` for the table and the
 * request, `write` for a change, and `validate` for the route, which keeps only
 * what `read` understood. A sort the API does not know or a `?page=abc` is
 * dropped from the canonical URL rather than left there unshown.
 */
export function tableSearch({
  variants,
  pageSize,
  normalise = (query) => query,
  defaultSort,
}: TableCodec) {
  const read = (search: TableSearch): DataTableQuery => {
    const params = new URLSearchParams();
    for (const key of KEYS) {
      const value = search[key];
      if (value) params.set(key, String(value));
    }
    return fromParams(params);
  };

  const fromParams = (params: URLSearchParams): DataTableQuery => {
    const parsed = parseTableQuery(params, variants, pageSize);
    // A larger page would have the table expect rows it never receives.
    if (parsed.pagination.pageSize > MAX_PAGE_SIZE) {
      parsed.pagination = { ...parsed.pagination, pageSize };
    }
    const query = normalise(parsed);
    if (query.sorting.length === 0 && defaultSort) {
      return { ...query, sorting: [defaultSort] };
    }
    return query;
  };

  const write = (query: DataTableQuery): TableSearch => {
    const normalised = normalise(query);
    const [first, ...rest] = normalised.sorting;
    const isDefault =
      defaultSort !== undefined &&
      rest.length === 0 &&
      first?.id === defaultSort.id &&
      first.desc === defaultSort.desc;
    const params = serializeTableQuery(
      isDefault ? { ...normalised, sorting: [] } : normalised,
      variants,
      pageSize,
    );
    return {
      q: params.get('q') ?? undefined,
      sort: params.get('sort') ?? undefined,
      page: num(params.get('page')),
      per: num(params.get('per')),
    };
  };

  const validate = (search: Record<string, unknown>): TableSearch => {
    const params = new URLSearchParams();
    for (const key of KEYS) {
      const value = raw(search[key]);
      if (value) params.set(key, value);
    }
    return write(fromParams(params));
  };

  return { read, write, validate };
}

/** One filter's single value: the first of an options filter, or its text. */
export function filterValue(
  query: DataTableQuery,
  id: string,
): string | undefined {
  const value = query.filters.find((f) => f.id === id)?.value;
  if (Array.isArray(value)) return value[0] ?? undefined;
  return typeof value === 'string' && value ? value : undefined;
}

/**
 * Where a stale or shared `?page=` past the end goes: the last page that
 * exists (`undefined`, the first, when none does), or `false` when the page is
 * real. An empty page under a positive count has no way back otherwise.
 */
export function pageBeyondEnd(
  page: number,
  totalPages: number,
): number | undefined | false {
  if (page <= Math.max(totalPages, 1)) return false;
  return totalPages > 1 ? totalPages : undefined;
}
