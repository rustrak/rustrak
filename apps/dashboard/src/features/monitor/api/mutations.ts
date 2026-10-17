import type {
  CreateMonitor,
  Monitor,
  Result,
  RustrakError,
  UpdateMonitor,
} from '@rustrak/client';
import { createClient } from '@/shared/api/rustrak';

export async function createMonitor(
  projectId: number,
  input: CreateMonitor,
): Promise<Result<Monitor, RustrakError>> {
  const client = await createClient();
  return client.monitors.create(projectId, input);
}

export async function updateMonitor(
  projectId: number,
  slug: string,
  input: UpdateMonitor,
): Promise<Result<Monitor, RustrakError>> {
  const client = await createClient();
  return client.monitors.update(projectId, slug, input);
}

export async function deleteMonitor(
  projectId: number,
  slug: string,
): Promise<Result<void, RustrakError>> {
  const client = await createClient();
  return client.monitors.delete(projectId, slug);
}
