import { Skeleton } from '@rustrak/ui';
import { noop, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { mcpQueries } from '@/features/mcp/api/queries';
import { ConnectedApps } from '@/features/mcp/ui/components/connected-apps';
import { McpSettings } from '@/features/mcp/ui/components/mcp-settings';
import { PageLoading } from '@/shared/ui/components/page-loading';
import {
  SettingRow,
  SettingSection,
  SettingsListSkeleton,
} from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
import { useSessionUser } from '@/shared/ui/hooks/use-session-user';

export const Route = createFileRoute('/_authenticated/settings/mcp')({
  // Whether the server is on is the page; who is connected arrives after.
  loader: ({ context: { queryClient } }) => {
    void queryClient.query(mcpQueries.connections()).catch(noop);
    void queryClient.query(mcpQueries.settings()).catch(noop);
  },
  pendingComponent: McpLoading,
  component: McpPage,
});

function McpPage() {
  const t = useTranslations('settings');
  const { data: settings } = useSuspenseQuery(mcpQueries.settings());
  const user = useSessionUser();
  return (
    <>
      <title>{t('mcp.meta.title')}</title>
      <McpSettings
        settings={settings}
        isAdmin={user.role === 'admin'}
        connected={<ConnectedApps />}
      />
    </>
  );
}

function McpLoading() {
  const t = useTranslations('settings');
  return (
    <PageLoading>
      <title>{t('mcp.meta.title')}</title>
      <SettingsHeader title={t('mcp.title')} description={t('mcp.subtitle')} />
      <SettingSection title={t('mcp.page.connect')}>
        <SettingRow title={t('mcp.server.endpoint')}>
          <Skeleton className="h-9 w-full rounded-lg" />
        </SettingRow>
      </SettingSection>
      <SettingSection title={t('mcp.connected.title')}>
        <SettingsListSkeleton rows={2} />
      </SettingSection>
    </PageLoading>
  );
}
