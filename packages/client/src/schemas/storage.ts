import { z } from 'zod';

/**
 * Exact source-map storage weight, summed from the `size` columns of the
 * in-DB chunk table and the on-disk source_file CAS.
 */
export const sourceMapStorageSchema = z.object({
  chunk_bytes: z.number(),
  source_file_bytes: z.number(),
  total_bytes: z.number(),
  file_count: z.number(),
});

/**
 * Instance-wide storage summary: row counts per data category, whole-DB size
 * (best-effort, backend-reported), and exact source-map weight.
 */
export const storageSummarySchema = z.object({
  total_db_size_bytes: z.number(),
  events_count: z.number(),
  transactions_count: z.number(),
  spans_count: z.number(),
  logs_count: z.number(),
  source_maps: sourceMapStorageSchema,
});

/**
 * Per-project storage breakdown (one row per project, including empty ones).
 * `estimated_bytes` estimates the JSON payload length the project holds,
 * from a sample of its newest rows scaled by the row count (spans, which
 * have no ingest time, are sampled in no particular order).
 */
export const projectStorageSchema = z.object({
  project_id: z.number(),
  project_name: z.string(),
  events_count: z.number(),
  transactions_count: z.number(),
  spans_count: z.number(),
  logs_count: z.number(),
  source_maps_count: z.number(),
  estimated_bytes: z.number(),
});

/**
 * Rows affected by a cleanup — used symmetrically for the dry-run preview
 * ("what would be removed") and the executed result ("what was removed").
 */
export const cleanupCountsSchema = z.object({
  events: z.number(),
  transactions: z.number(),
  spans: z.number(),
  logs: z.number(),
  issues_removed: z.number(),
});

/**
 * The instance's cleanup job. A cleanup runs in the background, one at a time:
 * `removed` holds the rows deleted so far while `running`, and the total once
 * it is `completed` or `failed`. `idle` means none has run since the server
 * started. Batches a failed run committed stay deleted.
 */
export const cleanupStatusSchema = z.object({
  state: z.enum(['idle', 'running', 'completed', 'failed']),
  removed: cleanupCountsSchema,
  started_at: z.string().nullable(),
  finished_at: z.string().nullable(),
  error: z.string().nullable(),
});

/**
 * Outcome of a source-map garbage collection: orphaned files removed from the
 * DB and unlinked from disk, plus the exact bytes freed.
 */
export const sourceMapGcResultSchema = z.object({
  files_removed: z.number(),
  bytes_freed: z.number(),
});

/**
 * Request body for a cleanup (preview or execute): remove data older than
 * `older_than_days`, optionally scoped to one project (omit for all) and to
 * specific data categories. The minimum of 1 mirrors the server contract — a
 * smaller window would wipe the whole dataset.
 *
 * The `include_*` flags select which data categories the cleanup acts on. Each
 * is optional and defaults to `true` server-side, so omitting all of them keeps
 * the "delete everything older than the cutoff" behaviour. `include_transactions`
 * also governs the cascaded spans, and `include_events` governs the emptied
 * issues that get removed alongside their events.
 */
export const cleanupOptionsSchema = z.object({
  older_than_days: z.number().int().min(1),
  project_id: z.number().int().optional(),
  include_events: z.boolean().optional(),
  include_transactions: z.boolean().optional(),
  include_logs: z.boolean().optional(),
});
