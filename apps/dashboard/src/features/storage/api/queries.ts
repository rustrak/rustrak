import type {
  AutoCleanup,
  ProjectStorage,
  Result,
  RustrakError,
  StorageSummary,
} from '@rustrak/client';
import { createClient } from '@/shared/api/rustrak';

/**
 * The two reads the storage page makes, and the only two callers are Server
 * Components.
 *
 * They used to sit in `storage.ts` under a single `'use server'` covering the
 * whole slice, which made both of them public POST endpoints. Nothing called
 * them from the browser, so that bought nothing and cost two endpoints.
 */

/** Instance-wide storage summary (counts + DB size + source-map weight). */
export async function getStorageSummary(): Promise<
  Result<StorageSummary, RustrakError>
> {
  const client = await createClient();
  return client.storage.getSummary();
}

/** Per-project storage breakdown (one row per project). */
export async function getStorageProjects(): Promise<
  Result<ProjectStorage[], RustrakError>
> {
  const client = await createClient();
  return client.storage.getProjects();
}

/** Automatic cleanup: settings, next run and last run. */
export async function getAutoCleanup(): Promise<
  Result<AutoCleanup, RustrakError>
> {
  const client = await createClient();
  return client.storage.getAutoCleanup();
}
