import type { IssuesListAllData } from '@rustrak/client';
import type { DataTableQuery, FilterVariants } from '@rustrak/ui';

/**
 * The issue table's vocabulary: its quick views, the fields the query bar
 * and the column headers filter on, and how one table query becomes one API
 * request. Pure, so the mapping the whole screen rests on is tested here and
 * not through a browser.
 */

/**
 * The tabs above the table, one per status. Who an issue is assigned to is a
 * filter in the bar, not a tab: it combines with any of these.
 */
export const ISSUE_VIEWS = ['open', 'resolved', 'muted', 'all'] as const;

export type IssueView = (typeof ISSUE_VIEWS)[number];

/** How each filterable column reads in the `q` string. */
export const ISSUE_FILTER_VARIANTS: FilterVariants = {
  level: 'options',
  priority: 'options',
  assignee: 'options',
  environment: 'options',
  release: 'options',
  firstSeen: 'options',
  lastSeen: 'options',
  events: 'range',
};

/** The windows the seen filters offer; the API reads the same strings. */
export const SEEN_WINDOWS = ['1h', '24h', '7d', '14d', '30d'] as const;

/** Column id to the API's sort name, for the columns the server can sort. */
const SORTS: Record<string, NonNullable<ListQuery['sort']>> = {
  lastSeen: 'last_seen',
  firstSeen: 'first_seen',
  events: 'event_count',
  priority: 'priority',
};

/** The filters whose values are a fixed set the server reads. */
const SEEN = new Set<string>(SEEN_WINDOWS);
const FIXED_VALUES: Record<string, ReadonlySet<string>> = {
  firstSeen: SEEN,
  lastSeen: SEEN,
};

/**
 * The query as the server will actually apply it. Whatever it would ignore
 * or refuse -- a second sort, a sort on a column it cannot sort, or a window it
 * cannot parse -- is dropped here, because the
 * address would otherwise keep it and claim a list the request never asked
 * for.
 */
export function withServerQuery(query: DataTableQuery): DataTableQuery {
  const sort = query.sorting.find((s) => Object.hasOwn(SORTS, s.id));
  const filters = query.filters.flatMap((filter) => {
    const allowed = FIXED_VALUES[filter.id];
    if (!allowed || !Array.isArray(filter.value)) return [filter];
    const value = filter.value.filter((v) => allowed.has(String(v)));
    return value.length ? [{ ...filter, value }] : [];
  });
  return {
    ...query,
    sorting: sort ? [sort] : [],
    filters,
  };
}

/** The list's parameters: the instance-wide list's, a project's plus `project`. */
type ListQuery = NonNullable<IssuesListAllData['query']>;

const STATUS: Record<IssueView, NonNullable<ListQuery['filter']>> = {
  open: 'open',
  resolved: 'resolved',
  muted: 'muted',
  all: 'all',
};

/** How one filter's values land in the request. */
type Apply = (params: ListQuery, values: unknown[]) => void;

const joined =
  (key: 'level' | 'priority'): Apply =>
  (params, values) => {
    if (values.length) params[key] = values.join(',');
  };

const single =
  (
    key: 'assigned' | 'environment' | 'release' | 'first_seen' | 'last_seen',
  ): Apply =>
  (params, [first]) => {
    if (first != null) params[key] = String(first);
  };

/** Column id to its request parameter. */
const FILTER_PARAMS: Record<string, Apply> = {
  level: joined('level'),
  priority: joined('priority'),
  assignee: single('assigned'),
  environment: single('environment'),
  release: single('release'),
  firstSeen: single('first_seen'),
  lastSeen: single('last_seen'),
  events: (params, [min, max]) => {
    if (min != null) params.events_min = min as number;
    if (max != null) params.events_max = max as number;
  },
};

/** One table query, in a view, as the list endpoint's parameters. */
export function issueListParams(
  query: DataTableQuery,
  view: IssueView,
): ListQuery {
  const params: ListQuery = { filter: STATUS[view] };

  if (query.search) params.q = query.search;

  for (const { id, value } of query.filters) {
    FILTER_PARAMS[id]?.(params, Array.isArray(value) ? value : []);
  }

  const sort = query.sorting[0];
  params.sort =
    (sort && Object.hasOwn(SORTS, sort.id) && SORTS[sort.id]) || 'last_seen';
  params.order = sort && !sort.desc ? 'asc' : 'desc';
  params.page = query.pagination.pageIndex + 1;
  params.per_page = query.pagination.pageSize;

  return params;
}

/**
 * A selection's ids, one batch per project: a change to issues of several
 * projects is one request to each project's endpoint.
 */
export function byProject(
  issues: readonly { id: string; project_id: number }[],
): Map<number, string[]> {
  const batches = new Map<number, string[]>();
  for (const { id, project_id } of issues) {
    batches.set(project_id, [...(batches.get(project_id) ?? []), id]);
  }
  return batches;
}
