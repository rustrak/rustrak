import type { z } from 'zod';
import type {
  apiErrorSchema,
  issueFilterSchema,
  issueSortSchema,
  sortOrderSchema,
} from '../schemas/common.js';

/**
 * Paginated response wrapper for list endpoints (cursor-based)
 */
export interface PaginatedResponse<T> {
  items: T[];
  next_cursor?: string;
  has_more: boolean;
}

/**
 * Offset-based paginated response wrapper for list endpoints
 */
export interface OffsetPaginatedResponse<T> {
  items: T[];
  total_count: number;
  page: number;
  per_page: number;
  total_pages: number;
}

/**
 * Sort order for list queries
 */
export type SortOrder = z.infer<typeof sortOrderSchema>;

/**
 * Sort field for issue queries
 */
export type IssueSort = z.infer<typeof issueSortSchema>;

/**
 * Filter for issue queries
 */
export type IssueFilter = z.infer<typeof issueFilterSchema>;

/**
 * The flat `{error, message?}` error body, as it appears **on the wire**.
 *
 * This is not the client's error type and no resource method returns it. A
 * failed call reports a `RustrakError` inside a `Result`; the two have no
 * relationship, and `RustrakError` is built by reading a body like this one,
 * not by exposing it.
 *
 * It is kept for code that reads a Rustrak error response outside this client
 * (a raw `fetch`, a proxy, a log line). Note what it models: the flat body the
 * ingest rate limiter sends (`routes/ingest.rs`). Every `AppError` from the
 * management API uses the nested `{error: {type, message}}` shape instead,
 * which this type does not describe.
 */
export type ApiError = z.infer<typeof apiErrorSchema>;

/**
 * List options for issues endpoint (offset-based pagination)
 */
export interface ListIssuesOptions {
  page?: number;
  per_page?: number;
  sort?: IssueSort;
  order?: SortOrder;
  filter?: IssueFilter;
  /** Free-text search across type, value, transaction, and culprit. */
  q?: string;
  environment?: string;
}

/**
 * List options for events endpoint
 */
export interface ListEventsOptions {
  order?: SortOrder;
  cursor?: string;
  environment?: string;
}

/**
 * List options for projects endpoint (offset-based pagination)
 */
export interface ListProjectsOptions {
  page?: number;
  per_page?: number;
  order?: SortOrder;
  /**
   * Attach per-project stats to each row, over this window (e.g. '24h', '7d').
   *
   * Omit it and no `stats` field comes back at all, which is what keeps the
   * plain list cheap for callers that only need names and DSNs.
   */
  stats_period?: string;
}

/**
 * List options for transactions endpoint (offset-based pagination + filters)
 */
export interface ListTransactionsOptions {
  page?: number;
  per_page?: number;
  /** Filter by exact transaction name (lists one group's samples). */
  name?: string;
  /** Filter by trace operation (contexts.trace.op), e.g. `http.server`. */
  op?: string;
  /** Filter by trace status (contexts.trace.status), e.g. `ok`. */
  status?: string;
  /** Filter by environment. */
  environment?: string;
  /** Filter by release. */
  release?: string;
}

/**
 * List options for logs endpoint (offset-based pagination + filters)
 */
export interface ListLogsOptions {
  page?: number;
  per_page?: number;
  /** Filter by log level (trace/debug/info/warn/error/fatal). */
  level?: string;
  /** Filter by trace id. */
  trace_id?: string;
  environment?: string;
}

/**
 * Options for the release health stats endpoint (offset-based pagination +
 * filters)
 */
export interface ReleaseHealthStatsOptions {
  page?: number;
  per_page?: number;
  /** Time window (e.g. '24h', '7d'). Defaults to '24h'. */
  period?: string;
  environment?: string;
  /**
   * Scope to a single release (all environments), computed server-side. Omit
   * to get every release in the project.
   */
  release?: string;
}
