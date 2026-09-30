import type { RustrakError } from '../errors.js';
import type { Result } from '../result.js';
import { offsetPaginatedResponseSchema } from '../schemas/common.js';
import {
  releaseHealthRowSchema,
  sessionSummarySchema,
  sessionTimeseriesSchema,
} from '../schemas/session.js';
import type {
  OffsetPaginatedResponse,
  ReleaseHealthStatsOptions,
} from '../types/common.js';
import type {
  ReleaseHealthRow,
  SessionSummary,
  SessionTimeseries,
} from '../types/session.js';
import { BaseResource } from './base.js';

/**
 * Sessions API resource — release health stats.
 */
export class SessionsResource extends BaseResource {
  /**
   * Get per-release health stats for a project, one offset-based page of
   * (release, environment) rows at a time, ordered by session volume.
   * @param projectId - Project ID
   * @param options - Time window, release scoping and pagination.
   */
  async stats(
    projectId: number,
    options?: ReleaseHealthStatsOptions,
  ): Promise<Result<OffsetPaginatedResponse<ReleaseHealthRow>, RustrakError>> {
    const searchParams: Record<string, string> = {};
    if (options?.period) {
      searchParams.period = options.period;
    }
    if (options?.release) {
      searchParams.release = options.release;
    }
    if (options?.environment) searchParams.environment = options.environment;
    if (options?.page) {
      searchParams.page = String(options.page);
    }
    if (options?.per_page) {
      searchParams.per_page = String(options.per_page);
    }

    return this.request(
      () =>
        this.http.get(`api/projects/${projectId}/sessions/stats`, {
          searchParams,
        }),
      offsetPaginatedResponseSchema(releaseHealthRowSchema),
    );
  }

  /**
   * Get project-wide session health, aggregated across all releases and environments.
   * @param projectId - Project ID
   * @param period - Time window (e.g. '24h', '7d'). Defaults to '24h'.
   */
  async summary(
    projectId: number,
    period?: string,
    environment?: string,
  ): Promise<Result<SessionSummary, RustrakError>> {
    const searchParams: Record<string, string> = {};
    if (period) {
      searchParams.period = period;
    }
    if (environment) searchParams.environment = environment;

    return this.request(
      () =>
        this.http.get(`api/projects/${projectId}/sessions/summary`, {
          searchParams,
        }),
      sessionSummarySchema,
    );
  }

  /**
   * Get a time-bucketed session trend for a project, aggregated across all
   * releases and environments.
   * @param projectId - Project ID
   * @param period - Time window (e.g. '24h', '7d'). Defaults to '24h'.
   * @param interval - Bucket width in hours (default: 1, max: 24).
   */
  async timeseries(
    projectId: number,
    period?: string,
    interval?: number,
    environment?: string,
  ): Promise<Result<SessionTimeseries, RustrakError>> {
    const searchParams: Record<string, string> = {};
    if (period) {
      searchParams.period = period;
    }
    if (interval !== undefined) {
      searchParams.interval = interval.toString();
    }
    if (environment) searchParams.environment = environment;

    return this.request(
      () =>
        this.http.get(`api/projects/${projectId}/sessions/timeseries`, {
          searchParams,
        }),
      sessionTimeseriesSchema,
    );
  }
}
