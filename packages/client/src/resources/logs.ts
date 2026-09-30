import type { RustrakError } from '../errors.js';
import type { Result } from '../result.js';
import { logSchema, offsetPaginatedResponseSchema } from '../schemas/index.js';
import type {
  ListLogsOptions,
  Log,
  OffsetPaginatedResponse,
} from '../types/index.js';
import { BaseResource } from './base.js';

/**
 * Logs API resource (standalone logs — Sentry "log" item type)
 */
export class LogsResource extends BaseResource {
  /**
   * List logs for a project with offset-based pagination (newest first by log
   * timestamp), optionally filtered by level/trace_id.
   */
  async list(
    projectId: number,
    options?: ListLogsOptions,
  ): Promise<Result<OffsetPaginatedResponse<Log>, RustrakError>> {
    const searchParams: Record<string, string> = {};

    if (options?.page) {
      searchParams.page = String(options.page);
    }
    if (options?.per_page) {
      searchParams.per_page = String(options.per_page);
    }
    if (options?.level) {
      searchParams.level = options.level;
    }
    if (options?.trace_id) {
      searchParams.trace_id = options.trace_id;
    }
    if (options?.environment) {
      searchParams.environment = options.environment;
    }

    return this.request(
      () => this.http.get(`api/projects/${projectId}/logs`, { searchParams }),
      offsetPaginatedResponseSchema(logSchema),
    );
  }
}
