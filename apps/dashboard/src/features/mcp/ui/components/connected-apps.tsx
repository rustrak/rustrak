import type { OAuthGrant } from '@rustrak/client';
import { Avatar, Button, IntegrationsIcon } from '@rustrak/ui';
import { useMutation, useQuery } from '@tanstack/react-query';
import { toast } from 'sonner';
import { useFormatter, useTranslations } from 'use-intl';
import { mcpMutations } from '@/features/mcp/api/mutations';
import { mcpQueries } from '@/features/mcp/api/queries';
import { QueryState } from '@/shared/ui/components/query-state';
import {
  SettingSection,
  SettingsEmpty,
  SettingsListItem,
} from '@/shared/ui/components/setting-row';
import { useToastFailure } from '@/shared/ui/hooks/use-toast-failure';

/** The host a client is published from, which is what a reader recognises. */
function clientHost(clientId: string): string {
  try {
    return new URL(clientId).host;
  } catch {
    return clientId;
  }
}

/**
 * The assistants connected to the reader's account by signing in, and the
 * one control that matters for each: cutting it off.
 */
export function ConnectedApps() {
  const t = useTranslations('settings');
  const toastFailure = useToastFailure();
  const format = useFormatter();
  const connections = useQuery(mcpQueries.connections());
  const revocation = useMutation(mcpMutations.revokeConnection());

  const revoke = (grant: OAuthGrant) =>
    revocation.mutate(grant.id, {
      onError: (error) => toastFailure(t('mcp.connected.revokeFailed'), error),
      onSuccess: () =>
        toast.success(t('mcp.connected.revoked', { name: grant.client_name })),
    });

  return (
    <SettingSection title={t('mcp.connected.title')}>
      <QueryState
        query={connections}
        placeholder={null}
        title={t('mcp.connected.loadFailed')}
      >
        {(grants) =>
          grants.length === 0 ? (
            <SettingsEmpty
              icon={<IntegrationsIcon size="2xl" />}
              title={t('mcp.connected.empty')}
            />
          ) : (
            grants.map((grant) => (
              <SettingsListItem
                key={grant.id}
                leading={<Avatar name={grant.client_name} shape="square" />}
                title={grant.client_name}
                description={
                  <span className="font-mono text-mono">
                    {clientHost(grant.client_id)}
                  </span>
                }
                meta={format.dateTime(new Date(grant.created_at), 'date')}
                actions={
                  <Button
                    variant="secondary"
                    size="sm"
                    disabled={revocation.isPending}
                    onClick={() => revoke(grant)}
                    aria-label={t('mcp.connected.revokeLabel', {
                      name: grant.client_name,
                    })}
                  >
                    {t('mcp.connected.revoke')}
                  </Button>
                }
              />
            ))
          )
        }
      </QueryState>
    </SettingSection>
  );
}
