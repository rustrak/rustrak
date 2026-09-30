import { z } from 'zod';
import type { RustrakError } from '../errors.js';
import type { Result } from '../result.js';
import {
  activityEntrySchema,
  bulkDeleteIssuesSchema,
  bulkUpdateIssuesSchema,
  createCommentSchema,
  createUserReportSchema,
  issueAggregatesSchema,
  issueHashSchema,
  issueSchema,
  issueStatsSchema,
  issueTagValueSchema,
  offsetPaginatedResponseSchema,
  updateIssueStateSchema,
  userReportSchema,
} from '../schemas/index.js';
import type {
  ActivityEntry,
  BulkDeleteIssues,
  BulkUpdateIssues,
  CreateComment,
  CreateUserReport,
  Issue,
  IssueAggregates,
  IssueHash,
  IssueStats,
  IssueStatsWindow,
  IssueTagValue,
  ListIssuesOptions,
  OffsetPaginatedResponse,
  UpdateIssueState,
  UserReport,
} from '../types/index.js';
import { BaseResource } from './base.js';

/**
 * Issues API resource
 */
export class IssuesResource extends BaseResource {
  /**
   * List issues for a project with offset-based pagination
   */
  async list(
    projectId: number,
    options?: ListIssuesOptions,
  ): Promise<Result<OffsetPaginatedResponse<Issue>, RustrakError>> {
    const searchParams: Record<string, string> = {};

    if (options?.page !== undefined) {
      searchParams.page = options.page.toString();
    }
    if (options?.per_page !== undefined) {
      searchParams.per_page = options.per_page.toString();
    }
    if (options?.sort) {
      searchParams.sort = options.sort;
    }
    if (options?.order) {
      searchParams.order = options.order;
    }
    if (options?.filter) {
      searchParams.filter = options.filter;
    }
    if (options?.q) {
      searchParams.q = options.q;
    }
    if (options?.environment) {
      searchParams.environment = options.environment;
    }

    return this.request(
      () => this.http.get(`api/projects/${projectId}/issues`, { searchParams }),
      offsetPaginatedResponseSchema(issueSchema),
    );
  }

  /**
   * Get a single issue by ID
   */
  async get(
    projectId: number,
    issueId: string,
    environment?: string,
  ): Promise<Result<Issue, RustrakError>> {
    return this.request(
      () =>
        this.http.get(`api/projects/${projectId}/issues/${issueId}`, {
          searchParams: environment ? { environment } : {},
        }),
      issueSchema,
    );
  }

  /**
   * Update issue state (status, substatus, priority, assignment, …)
   */
  async updateState(
    projectId: number,
    issueId: string,
    input: UpdateIssueState,
  ): Promise<Result<Issue, RustrakError>> {
    const validatedInput = this.validateInput(input, updateIssueStateSchema);
    if (!validatedInput.success) {
      return validatedInput;
    }

    return this.request(
      () =>
        this.http.patch(`api/projects/${projectId}/issues/${issueId}`, {
          json: validatedInput.data,
        }),
      issueSchema,
    );
  }

  /**
   * Resolve an issue in the next release (suppresses regression until a deploy).
   */
  async resolveInNextRelease(
    projectId: number,
    issueId: string,
  ): Promise<Result<Issue, RustrakError>> {
    return this.updateState(projectId, issueId, {
      status: 'resolvedInNextRelease',
    });
  }

  /**
   * Delete an issue
   */
  async delete(
    projectId: number,
    issueId: string,
  ): Promise<Result<void, RustrakError>> {
    return this.requestVoid(() =>
      this.http.delete(`api/projects/${projectId}/issues/${issueId}`),
    );
  }

  /**
   * Bulk-mutate issues (status and/or priority).
   */
  async bulkUpdate(
    projectId: number,
    input: BulkUpdateIssues,
  ): Promise<Result<{ updated: number }, RustrakError>> {
    const validatedInput = this.validateInput(input, bulkUpdateIssuesSchema);
    if (!validatedInput.success) {
      return validatedInput;
    }

    return this.request(
      () =>
        this.http.put(`api/projects/${projectId}/issues`, {
          json: validatedInput.data,
        }),
      z.object({ updated: z.number().int() }),
    );
  }

  /**
   * Bulk-delete issues.
   */
  async bulkDelete(
    projectId: number,
    input: BulkDeleteIssues,
  ): Promise<Result<{ deleted: number }, RustrakError>> {
    const validatedInput = this.validateInput(input, bulkDeleteIssuesSchema);
    if (!validatedInput.success) {
      return validatedInput;
    }

    return this.request(
      () =>
        this.http.delete(`api/projects/${projectId}/issues`, {
          json: validatedInput.data,
        }),
      z.object({ deleted: z.number().int() }),
    );
  }

  /**
   * List the grouping hashes that map to an issue.
   */
  async getHashes(
    projectId: number,
    issueId: string,
  ): Promise<Result<IssueHash[], RustrakError>> {
    return this.request(
      () => this.http.get(`api/projects/${projectId}/issues/${issueId}/hashes`),
      z.array(issueHashSchema),
    );
  }

  /**
   * List the distinct values (with counts) for a tag key across the issue.
   * Returns a bare list, one entry per value (Sentry-compatible shape).
   */
  async getTagValues(
    projectId: number,
    issueId: string,
    key: string,
    environment?: string,
  ): Promise<Result<IssueTagValue[], RustrakError>> {
    return this.request(
      () =>
        this.http.get(
          `api/projects/${projectId}/issues/${issueId}/tags/${encodeURIComponent(key)}`,
          { searchParams: environment ? { environment } : {} },
        ),
      z.array(issueTagValueSchema),
    );
  }

  /**
   * Fetch per-issue aggregates (unique user count + top tags).
   */
  async getAggregates(
    projectId: number,
    issueId: string,
    environment?: string,
  ): Promise<Result<IssueAggregates, RustrakError>> {
    return this.request(
      () =>
        this.http.get(
          `api/projects/${projectId}/issues/${issueId}/aggregates`,
          { searchParams: environment ? { environment } : {} },
        ),
      issueAggregatesSchema,
    );
  }

  /**
   * Fetch a zero-filled event-count timeseries (24h or 30d).
   */
  async getStats(
    projectId: number,
    issueId: string,
    window: IssueStatsWindow = '24h',
    environment?: string,
  ): Promise<Result<IssueStats, RustrakError>> {
    return this.request(
      () =>
        this.http.get(`api/projects/${projectId}/issues/${issueId}/stats`, {
          searchParams: { window, ...(environment ? { environment } : {}) },
        }),
      issueStatsSchema,
    );
  }

  /**
   * List an issue's activity log (status changes, comments/notes, …).
   */
  async getActivity(
    projectId: number,
    issueId: string,
  ): Promise<Result<ActivityEntry[], RustrakError>> {
    return this.request(
      () =>
        this.http.get(`api/projects/${projectId}/issues/${issueId}/activity`),
      z.array(activityEntrySchema),
    );
  }

  /**
   * Add a comment (note) to an issue.
   */
  async addComment(
    projectId: number,
    issueId: string,
    input: CreateComment,
  ): Promise<Result<ActivityEntry, RustrakError>> {
    const validatedInput = this.validateInput(input, createCommentSchema);
    if (!validatedInput.success) {
      return validatedInput;
    }

    return this.request(
      () =>
        this.http.post(`api/projects/${projectId}/issues/${issueId}/comments`, {
          json: validatedInput.data,
        }),
      activityEntrySchema,
    );
  }

  /**
   * Set or clear the current user's bookmark on an issue.
   */
  async setBookmark(
    projectId: number,
    issueId: string,
    enabled: boolean,
  ): Promise<Result<{ is_bookmarked: boolean }, RustrakError>> {
    return this.request(
      () =>
        this.http.put(`api/projects/${projectId}/issues/${issueId}/bookmark`, {
          json: { enabled },
        }),
      z.object({ is_bookmarked: z.boolean() }),
    );
  }

  /**
   * Set or clear the current user's subscription to an issue.
   */
  async setSubscription(
    projectId: number,
    issueId: string,
    enabled: boolean,
  ): Promise<Result<{ is_subscribed: boolean }, RustrakError>> {
    return this.request(
      () =>
        this.http.put(
          `api/projects/${projectId}/issues/${issueId}/subscription`,
          { json: { enabled } },
        ),
      z.object({ is_subscribed: z.boolean() }),
    );
  }

  /**
   * Mark the issue as seen by the current user.
   */
  async markSeen(
    projectId: number,
    issueId: string,
  ): Promise<Result<{ has_seen: boolean }, RustrakError>> {
    return this.request(
      () => this.http.post(`api/projects/${projectId}/issues/${issueId}/seen`),
      z.object({ has_seen: z.boolean() }),
    );
  }

  /**
   * List user feedback reports for an issue.
   */
  async listUserReports(
    projectId: number,
    issueId: string,
  ): Promise<Result<UserReport[], RustrakError>> {
    return this.request(
      () =>
        this.http.get(
          `api/projects/${projectId}/issues/${issueId}/user-reports`,
        ),
      z.array(userReportSchema),
    );
  }

  /**
   * Attach a user feedback report to an issue.
   */
  async createUserReport(
    projectId: number,
    issueId: string,
    input: CreateUserReport,
  ): Promise<Result<UserReport, RustrakError>> {
    const validatedInput = this.validateInput(input, createUserReportSchema);
    if (!validatedInput.success) {
      return validatedInput;
    }

    return this.request(
      () =>
        this.http.post(
          `api/projects/${projectId}/issues/${issueId}/user-reports`,
          { json: validatedInput.data },
        ),
      userReportSchema,
    );
  }
}
