import { z } from 'zod';
import type { RustrakError } from '../errors.js';
import type { Result } from '../result.js';
import {
  cleanupCountsSchema,
  cleanupOptionsSchema,
  cleanupStatusSchema,
  projectStorageSchema,
  sourceMapGcResultSchema,
  storageSummarySchema,
} from '../schemas/index.js';
import type {
  CleanupCounts,
  CleanupOptions,
  CleanupStatus,
  ProjectStorage,
  SourceMapGcResult,
  StorageSummary,
} from '../types/index.js';
import { BaseResource } from './base.js';

/**
 * Storage API resource (admin only).
 *
 * Surfaces how much data the instance is holding and runs retention cleanups.
 */
export class StorageResource extends BaseResource {
  /**
   * Get the instance-wide storage summary: row counts per data category, the
   * whole-DB size, and exact source-map weight.
   */
  async getSummary(): Promise<Result<StorageSummary, RustrakError>> {
    return this.request(
      () => this.http.get('api/storage/summary'),
      storageSummarySchema,
    );
  }

  /**
   * Get the per-project storage breakdown (one row per project, including
   * empty ones).
   */
  async getProjects(): Promise<Result<ProjectStorage[], RustrakError>> {
    return this.request(
      () => this.http.get('api/storage/projects'),
      z.array(projectStorageSchema),
    );
  }

  /**
   * Dry-run: count the rows a cleanup would remove. Mutates nothing — use it to
   * confirm impact before {@link executeCleanup}.
   */
  async previewCleanup(
    options: CleanupOptions,
  ): Promise<Result<CleanupCounts, RustrakError>> {
    const validatedInput = this.validateInput(options, cleanupOptionsSchema);
    if (!validatedInput.success) {
      return validatedInput;
    }

    return this.request(
      () =>
        this.http.post('api/storage/cleanup/preview', {
          json: validatedInput.data,
        }),
      cleanupCountsSchema,
    );
  }

  /**
   * Start a cleanup: delete data older than `older_than_days` (optionally
   * scoped to one project) and remove the issues it leaves with zero events.
   *
   * Returns as soon as the server has started it, with the `running` status.
   * The deletion runs in the background; follow it with
   * {@link getCleanupStatus}. A second start while one is running fails with
   * a `conflict`.
   */
  async executeCleanup(
    options: CleanupOptions,
  ): Promise<Result<CleanupStatus, RustrakError>> {
    const validatedInput = this.validateInput(options, cleanupOptionsSchema);
    if (!validatedInput.success) {
      return validatedInput;
    }

    return this.request(
      () =>
        this.http.post('api/storage/cleanup', {
          json: validatedInput.data,
          // No retry: if the first attempt started the cleanup and only its
          // response was lost, a retry would be refused as a second run and
          // report a failure for a cleanup that is in fact running.
          retry: 0,
        }),
      cleanupStatusSchema,
    );
  }

  /**
   * The running cleanup's progress, or the outcome of the last one.
   */
  async getCleanupStatus(): Promise<Result<CleanupStatus, RustrakError>> {
    return this.request(
      () => this.http.get('api/storage/cleanup/status'),
      cleanupStatusSchema,
    );
  }

  /**
   * Dry-run for {@link gcSourceMaps}: count the orphaned source-map files and
   * bytes a GC would reclaim. Mutates nothing.
   */
  async previewGcSourceMaps(): Promise<
    Result<SourceMapGcResult, RustrakError>
  > {
    return this.request(
      () => this.http.post('api/storage/source-maps/gc/preview'),
      sourceMapGcResultSchema,
    );
  }

  /**
   * Garbage-collect orphaned source maps: files no longer referenced by any
   * upload, removed from the DB and unlinked from disk. Safe — never touches
   * referenced files.
   */
  async gcSourceMaps(): Promise<Result<SourceMapGcResult, RustrakError>> {
    return this.request(
      () => this.http.post('api/storage/source-maps/gc'),
      sourceMapGcResultSchema,
    );
  }
}
