import { z } from 'zod';
import type { RustrakError } from '../errors.js';
import type { Result } from '../result.js';
import { offsetPaginatedResponseSchema } from '../schemas/common.js';
import {
  checkInSchema,
  createMonitorSchema,
  monitorSchema,
  updateMonitorSchema,
} from '../schemas/monitor.js';
import type { OffsetPaginatedResponse } from '../types/index.js';
import type {
  CheckIn,
  CreateMonitor,
  ListCheckInsOptions,
  Monitor,
  UpdateMonitor,
} from '../types/monitor.js';
import { BaseResource } from './base.js';

/**
 * Cron monitors API resource (Sentry Crons). Monitors are addressed by slug,
 * the same slug SDKs check in with.
 */
export class MonitorsResource extends BaseResource {
  /** Every monitor of a project, with its environments and recent check-ins. */
  async list(projectId: number): Promise<Result<Monitor[], RustrakError>> {
    return this.request(
      () => this.http.get(`api/projects/${projectId}/monitors`),
      z.array(monitorSchema),
    );
  }

  async get(
    projectId: number,
    slug: string,
  ): Promise<Result<Monitor, RustrakError>> {
    return this.request(
      () =>
        this.http.get(
          `api/projects/${projectId}/monitors/${encodeURIComponent(slug)}`,
        ),
      monitorSchema,
    );
  }

  async create(
    projectId: number,
    input: CreateMonitor,
  ): Promise<Result<Monitor, RustrakError>> {
    const validated = this.validateInput(input, createMonitorSchema);
    if (!validated.success) {
      return validated;
    }
    return this.request(
      () =>
        this.http.post(`api/projects/${projectId}/monitors`, {
          json: validated.data,
        }),
      monitorSchema,
    );
  }

  /** Fields left out keep their value, including each field of `config`. */
  async update(
    projectId: number,
    slug: string,
    input: UpdateMonitor,
  ): Promise<Result<Monitor, RustrakError>> {
    const validated = this.validateInput(input, updateMonitorSchema);
    if (!validated.success) {
      return validated;
    }
    return this.request(
      () =>
        this.http.put(
          `api/projects/${projectId}/monitors/${encodeURIComponent(slug)}`,
          { json: validated.data },
        ),
      monitorSchema,
    );
  }

  /** Deletes the monitor and its check-ins. Its issues stay. */
  async delete(
    projectId: number,
    slug: string,
  ): Promise<Result<void, RustrakError>> {
    return this.requestVoid(() =>
      this.http.delete(
        `api/projects/${projectId}/monitors/${encodeURIComponent(slug)}`,
      ),
    );
  }

  /** Check-ins of one monitor, newest first. */
  async listCheckIns(
    projectId: number,
    slug: string,
    options?: ListCheckInsOptions,
  ): Promise<Result<OffsetPaginatedResponse<CheckIn>, RustrakError>> {
    const searchParams: Record<string, string> = {};
    if (options?.page) {
      searchParams.page = String(options.page);
    }
    if (options?.per_page) {
      searchParams.per_page = String(options.per_page);
    }
    if (options?.environment) {
      searchParams.environment = options.environment;
    }
    return this.request(
      () =>
        this.http.get(
          `api/projects/${projectId}/monitors/${encodeURIComponent(slug)}/checkins`,
          { searchParams },
        ),
      offsetPaginatedResponseSchema(checkInSchema),
    );
  }
}
