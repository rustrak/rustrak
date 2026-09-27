/**
 * Reads for the alert feature: the rules a project has, and the integrations
 * they route to. A rule without an integration does nothing, which is why the
 * two live in one slice.
 */

import type {
  AlertIntegration,
  AlertRule,
  Result,
  RustrakError,
} from '@rustrak/client';
import { queryOptions } from '@tanstack/react-query';
import { scope } from '@/shared/api/query-client';
import { createClient } from '@/shared/api/rustrak';

// ============================================================================
// Alert Integrations (Global Credential Destinations)
// ============================================================================

export async function listIntegrations(): Promise<
  Result<AlertIntegration[], RustrakError>
> {
  const client = await createClient();
  return client.alertIntegrations.list();
}

export async function listAlertRules(
  projectId: number,
): Promise<Result<AlertRule[], RustrakError>> {
  const client = await createClient();
  return client.alertRules.list(projectId);
}

export const alertQueries = {
  integrations: () =>
    queryOptions({
      queryKey: scope.integrations,
      queryFn: listIntegrations,
    }),
  rules: (projectId: number) =>
    queryOptions({
      queryKey: [...scope.project(projectId), 'alert-rules'],
      queryFn: () => listAlertRules(projectId),
    }),
};
