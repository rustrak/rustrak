import type {
  CheckIn,
  ListCheckInsOptions,
  Monitor,
  OffsetPaginatedResponse,
  Result,
  RustrakError,
} from '@rustrak/client';
import { createClient } from '@/shared/api/rustrak';

export async function listMonitors(
  projectId: number,
): Promise<Result<Monitor[], RustrakError>> {
  const client = await createClient();
  return client.monitors.list(projectId);
}

export async function getMonitor(
  projectId: number,
  slug: string,
): Promise<Result<Monitor, RustrakError>> {
  const client = await createClient();
  return client.monitors.get(projectId, slug);
}

export async function listCheckIns(
  projectId: number,
  slug: string,
  options?: ListCheckInsOptions,
): Promise<Result<OffsetPaginatedResponse<CheckIn>, RustrakError>> {
  const client = await createClient();
  return client.monitors.listCheckIns(projectId, slug, options);
}
