import { z } from 'zod';
import { dateTimeSchema, issueFilterSchema, uuidSchema } from './common.js';

/**
 * Canonical issue status values (Sentry-compatible).
 */
export const issueStatusSchema = z.enum(['unresolved', 'resolved', 'ignored']);

/**
 * Issue priority tiers.
 */
export const issuePrioritySchema = z.enum(['low', 'medium', 'high']);

/**
 * Issue response schema from API
 */
export const issueSchema = z.object({
  id: uuidSchema,
  project_id: z.number().int(),
  short_id: z.string(),
  title: z.string(),
  value: z.string(),
  culprit: z.string(),
  logger: z.string(),
  first_seen: dateTimeSchema,
  last_seen: dateTimeSchema,
  event_count: z.number().int(),
  level: z.string().nullable(),
  platform: z.string().nullable(),
  // Status model (#165)
  status: issueStatusSchema,
  substatus: z.string().nullable(),
  priority: issuePrioritySchema.nullable(),
  assigned_to: z.number().int().nullable(),
  assignee_type: z.string().nullable(),
  issue_type: z.string(),
  issue_category: z.string(),
  first_release: z.string(),
  last_release: z.string(),
  status_details: z.record(z.string(), z.unknown()),
  // Aggregate enrichment — only the single-issue GET populates this; the list
  // and PATCH endpoints return the lean response, so default to 0 when absent.
  user_report_count: z.number().int().optional().default(0),
  // Per-user fields — only present when the request carries a user session.
  is_bookmarked: z.boolean().optional(),
  is_subscribed: z.boolean().optional(),
  has_seen: z.boolean().optional(),
  // Bulk list stats — only the list endpoint populates these (computed once
  // per page); the single-issue GET and PATCH responses omit them.
  user_count: z.number().int().optional(),
  trend: z.array(z.number().int()).optional(),
  // Deprecated, derived from `status`; kept for backward compatibility.
  is_resolved: z.boolean(),
  is_muted: z.boolean(),
});

/**
 * Update issue state request schema.
 *
 * `status` (plus `substatus`/`priority`/assignment) is canonical;
 * `is_resolved`/`is_muted` remain as a deprecated compatibility shim.
 * `status` also accepts the special `resolvedInNextRelease` value.
 */
export const updateIssueStateSchema = z.object({
  status: z
    .union([issueStatusSchema, z.literal('resolvedInNextRelease')])
    .optional(),
  substatus: z.string().optional(),
  priority: issuePrioritySchema.optional(),
  assigned_to: z.number().int().nullable().optional(),
  assignee_type: z.string().nullable().optional(),
  is_resolved: z.boolean().optional(),
  is_muted: z.boolean().optional(),
});

/**
 * A grouping hash that maps to an issue.
 */
export const issueHashSchema = z.object({
  id: z.number().int(),
  project_id: z.number().int(),
  issue_id: uuidSchema,
  grouping_key: z.string(),
  grouping_key_hash: z.string(),
  created_at: dateTimeSchema,
});

/**
 * A tag value with its event count.
 */
export const tagValueCountSchema = z.object({
  value: z.string(),
  count: z.number().int(),
});

/**
 * A single tag value's usage within an issue — the shape `GET
 * /issues/{id}/tags/{key}` returns per entry (Sentry-compatible: a bare
 * list of these, not a `{key, values}` wrapper).
 */
export const issueTagValueSchema = z.object({
  key: z.string(),
  name: z.string(),
  value: z.string(),
  count: z.number().int(),
  first_seen: dateTimeSchema,
  last_seen: dateTimeSchema,
});

/**
 * A tag key with its most common values within an issue.
 */
export const tagSummarySchema = z.object({
  key: z.string(),
  total_values: z.number().int(),
  top_values: z.array(tagValueCountSchema),
});

/**
 * Per-issue aggregates: unique user count + top tags.
 */
export const issueAggregatesSchema = z.object({
  user_count: z.number().int(),
  tags: z.array(tagSummarySchema),
});

/**
 * Time window for issue stats.
 */
export const issueStatsWindowSchema = z.enum(['24h', '30d']);

/**
 * Event-count timeseries for an issue: `[bucketStartUnix, count]` points.
 */
export const issueStatsSchema = z.object({
  data: z.array(z.tuple([z.number(), z.number()])),
});

/**
 * An entry in an issue's activity log (status changes, comments/notes, etc.).
 */
export const activityEntrySchema = z.object({
  id: uuidSchema,
  issue_id: uuidSchema,
  user_id: z.number().int().nullable(),
  type: z.string(),
  data: z.string(),
  created_at: dateTimeSchema,
});

/**
 * A user feedback report attached to an issue/event.
 */
export const userReportSchema = z.object({
  id: uuidSchema,
  project_id: z.number().int(),
  issue_id: uuidSchema.nullable(),
  event_id: uuidSchema.nullable(),
  name: z.string(),
  // Server stores '' for anonymous reports (column DEFAULT '' / unwrap_or_default),
  // matching real Sentry's own save_userreport(report.get("email", "")) — accept it.
  email: z.union([z.email(), z.literal('')]),
  comments: z.string(),
  created_at: dateTimeSchema,
});

/**
 * Request to add a comment (note) to an issue.
 */
export const createCommentSchema = z.object({
  text: z.string(),
});

/**
 * Request to submit user feedback on an issue.
 */
export const createUserReportSchema = z.object({
  name: z.string().optional(),
  email: z.email().optional(),
  comments: z.string().optional(),
  event_id: uuidSchema.optional(),
});

/**
 * Request to bulk-mutate issues.
 */
export const bulkUpdateIssuesSchema = z.object({
  ids: z.array(uuidSchema),
  status: z
    .union([issueStatusSchema, z.literal('resolvedInNextRelease')])
    .optional(),
  priority: issuePrioritySchema.optional(),
});

/**
 * Request to bulk-delete issues: the listed ids, or every issue matching a
 * filter. The server rejects a body with both or neither.
 */
export const bulkDeleteIssuesSchema = z.union([
  z.strictObject({ ids: z.array(uuidSchema) }),
  z.strictObject({ filter: issueFilterSchema }),
]);
