/**
 * Reads for the log feature.
 */

import type {
  ListLogsOptions,
  Log,
  OffsetPaginatedResponse,
  Result,
  RustrakError,
} from '@rustrak/client';
import { queryOptions } from '@tanstack/react-query';
import { scope } from '@/shared/api/query-client';
import { createClient } from '@/shared/api/rustrak';

export async function listLogs(
  projectId: number,
  options?: ListLogsOptions,
): Promise<Result<OffsetPaginatedResponse<Log>, RustrakError>> {
  const client = await createClient();
  return client.logs.list(projectId, options);
}

export const logQueries = {
  list: (projectId: number, options?: ListLogsOptions) =>
    queryOptions({
      queryKey: [...scope.project(projectId), 'logs', options],
      queryFn: () => listLogs(projectId, options),
    }),
};
