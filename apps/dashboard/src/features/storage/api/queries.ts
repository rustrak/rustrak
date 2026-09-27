import type {
  ProjectStorage,
  Result,
  RustrakError,
  StorageSummary,
} from '@rustrak/client';
import { queryOptions } from '@tanstack/react-query';
import { scope } from '@/shared/api/query-client';
import { createClient } from '@/shared/api/rustrak';

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

export const storageQueries = {
  summary: () =>
    queryOptions({
      queryKey: [...scope.storage, 'summary'],
      queryFn: getStorageSummary,
    }),
  projects: () =>
    queryOptions({
      queryKey: [...scope.storage, 'projects'],
      queryFn: getStorageProjects,
    }),
};
