import type { RustrakError } from '../errors.js';
import type { Result } from '../result.js';
import {
  createProjectSchema,
  offsetPaginatedResponseSchema,
  projectSchema,
  rateLimitsSchema,
  updateProjectSchema,
} from '../schemas/index.js';
import type {
  CreateProject,
  ListProjectsOptions,
  OffsetPaginatedResponse,
  Project,
  RateLimits,
  UpdateProject,
} from '../types/index.js';
import { BaseResource } from './base.js';

/**
 * Projects API resource
 */
export class ProjectsResource extends BaseResource {
  /**
   * List projects with pagination
   */
  async list(
    options?: ListProjectsOptions,
  ): Promise<Result<OffsetPaginatedResponse<Project>, RustrakError>> {
    const searchParams = new URLSearchParams();

    if (options?.page !== undefined) {
      searchParams.set('page', options.page.toString());
    }
    if (options?.per_page !== undefined) {
      searchParams.set('per_page', options.per_page.toString());
    }
    if (options?.order) {
      searchParams.set('order', options.order);
    }
    if (options?.stats_period) {
      searchParams.set('stats_period', options.stats_period);
    }

    const query = searchParams.toString();
    const url = query ? `api/projects?${query}` : 'api/projects';

    return this.request(
      () => this.http.get(url),
      offsetPaginatedResponseSchema(projectSchema),
    );
  }

  /**
   * The server's per-project limits, which a project without its own follows
   */
  async rateLimits(): Promise<Result<RateLimits, RustrakError>> {
    return this.request(
      () => this.http.get('api/rate-limits'),
      rateLimitsSchema,
    );
  }

  /**
   * Get a single project by ID
   */
  async get(id: number): Promise<Result<Project, RustrakError>> {
    return this.request(
      () => this.http.get(`api/projects/${id}`),
      projectSchema,
    );
  }

  /**
   * Create a new project
   */
  async create(input: CreateProject): Promise<Result<Project, RustrakError>> {
    const validatedInput = this.validateInput(input, createProjectSchema);
    if (!validatedInput.success) {
      return validatedInput;
    }

    return this.request(
      () => this.http.post('api/projects', { json: validatedInput.data }),
      projectSchema,
    );
  }

  /**
   * Update an existing project
   */
  async update(
    id: number,
    input: UpdateProject,
  ): Promise<Result<Project, RustrakError>> {
    const validatedInput = this.validateInput(input, updateProjectSchema);
    if (!validatedInput.success) {
      return validatedInput;
    }

    return this.request(
      () =>
        this.http.patch(`api/projects/${id}`, { json: validatedInput.data }),
      projectSchema,
    );
  }

  /**
   * Delete a project
   */
  async delete(id: number): Promise<Result<void, RustrakError>> {
    return this.requestVoid(() => this.http.delete(`api/projects/${id}`));
  }
}
