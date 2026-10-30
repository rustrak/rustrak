import { ExternalLinkIcon } from '@rustrak/ui';
import { noop, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { tokenQueries } from '@/features/token/api/queries';
import { TokensList } from '@/features/token/ui/components/tokens-list/tokens-list';
import { PageLoading } from '@/shared/ui/components/page-loading';
import {
  SettingSection,
  SettingsListSkeleton,
} from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';

export const Route = createFileRoute('/_authenticated/settings/tokens')({
  loader: ({ context: { queryClient } }) => {
    void queryClient.query(tokenQueries.list()).catch(noop);
  },
  pendingComponent: TokensLoading,
  component: TokensPage,
});

function TokensPage() {
  const t = useTranslations('settings');
  const { data: tokens } = useSuspenseQuery(tokenQueries.list());

  return (
    <>
      <title>{t('tokens.meta.title')}</title>
      <TokensList
        tokens={tokens}
        title={t('tokens.title')}
        subtitle={t('tokens.subtitle')}
        footer={
          <p className="mt-3 text-meta text-fg-subtle">
            {t('tokens.apiReferenceDescription')}{' '}
            <a
              href="https://rustrak.dev/api-reference"
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1 text-fg-brand hover:underline"
            >
              {t('tokens.apiReference')}
              <ExternalLinkIcon size="sm" aria-hidden="true" />
            </a>
          </p>
        }
      />
    </>
  );
}

function TokensLoading() {
  const t = useTranslations('settings');
  const tTokens = useTranslations('tokens');
  return (
    <PageLoading>
      <title>{t('tokens.meta.title')}</title>
      <SettingsHeader
        title={t('tokens.title')}
        description={t('tokens.subtitle')}
      />
      <SettingSection
        title={tTokens('list.title')}
        description={tTokens('list.description')}
      >
        <SettingsListSkeleton />
      </SettingSection>
    </PageLoading>
  );
}
