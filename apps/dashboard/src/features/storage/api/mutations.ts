import type {
  CleanupCounts,
  CleanupOptions,
  CleanupStatus,
  Result,
  RustrakError,
  SourceMapGcResult,
} from '@rustrak/client';
import { createClient } from '@/shared/api/rustrak';

/**
 * Everything the storage page's client components call.
 *
 * The two `preview*` functions mutate nothing, so they read oddly in a file
 * named `mutations.ts`. They belong here all the same: the split is by **who
 * calls it**, not by what it does to the database. `storage-cleanup.tsx` and
 * `source-map-gc.tsx` are `'use client'` and invoke them from the browser, so
 * they need the directive -- a `server-only` module would not be reachable.
 *
 * Both are POST on the server too, because a preview takes a `CleanupOptions`
 * body it has to compute against.
 */

/** Dry-run: count what a cleanup would remove. Mutates nothing. */
export async function previewStorageCleanup(
  options: CleanupOptions,
): Promise<Result<CleanupCounts, RustrakError>> {
  const client = await createClient();
  return client.storage.previewCleanup(options);
}

/**
 * Start a cleanup: permanently delete old data and emptied issues. Returns once
 * the server has started it; follow it with {@link getStorageCleanupStatus}.
 */
export async function executeStorageCleanup(
  options: CleanupOptions,
): Promise<Result<CleanupStatus, RustrakError>> {
  const client = await createClient();
  return client.storage.executeCleanup(options);
}

/** The running cleanup's progress, or the outcome of the last one. */
export async function getStorageCleanupStatus(): Promise<
  Result<CleanupStatus, RustrakError>
> {
  const client = await createClient();
  return client.storage.getCleanupStatus();
}

/** Dry-run: count orphaned source maps a GC would remove. Mutates nothing. */
export async function previewStorageSourceMapGc(): Promise<
  Result<SourceMapGcResult, RustrakError>
> {
  const client = await createClient();
  return client.storage.previewGcSourceMaps();
}

/** Garbage-collect orphaned source maps (DB rows + disk files). */
export async function gcStorageSourceMaps(): Promise<
  Result<SourceMapGcResult, RustrakError>
> {
  const client = await createClient();
  return client.storage.gcSourceMaps();
}
