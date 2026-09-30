import { z } from 'zod';
import type { RustrakError } from '../errors.js';
import type { Result } from '../result.js';
import { issueSchema } from '../schemas/index.js';
import type { Issue } from '../types/index.js';
import { BaseResource } from './base.js';

/**
 * Releases API resource — data scoped to a specific release.
 */
export class ReleasesResource extends BaseResource {
  /**
   * Get issues first seen in a given release, most recently introduced first.
   *
   * @param projectId - Project ID
   * @param release - Release version
   * @param limit - Max issues to return (default: 10, max: 50)
   */
  async newIssues(
    projectId: number,
    release: string,
    limit?: number,
    environment?: string,
  ): Promise<Result<Issue[], RustrakError>> {
    const searchParams: Record<string, string> = {};
    if (limit !== undefined) {
      searchParams.limit = limit.toString();
    }
    if (environment) searchParams.environment = environment;

    return this.request(
      () =>
        this.http.get(
          `api/projects/${projectId}/releases/${encodeURIComponent(release)}/new-issues`,
          { searchParams },
        ),
      z.array(issueSchema),
    );
  }
}
