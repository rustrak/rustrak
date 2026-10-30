import { Button, Tag, type TagTone } from '@rustrak/ui';
import { noop, useQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import type { ReactNode } from 'react';
import { useTranslations } from 'use-intl';
import { alertQueries } from '@/features/alert/api/queries';
import { alertProviders } from '@/features/alert/model/providers';
import { ProviderIcon } from '@/features/alert/ui/components/provider-icon';
import { githubQueries } from '@/features/github/api/queries';
import { cn } from '@/shared/lib/utils';
import { GitHubMark } from '@/shared/ui/components/github-mark';
import { Link } from '@/shared/ui/components/link';
import { LoadFailure } from '@/shared/ui/components/load-failure';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
import {
  integrationProjectId,
  integrationsBackHref,
} from '../-components/integrations-back-link';

export const Route = createFileRoute('/_authenticated/settings/integrations/')({
  validateSearch: (search: Record<string, unknown>) => ({
    projectId: integrationProjectId(search.projectId),
  }),
  // The catalogue stands without either read, and a card's status fills in
  // when its read lands or says which one failed.
  loader: ({ context: { queryClient } }) => {
    void queryClient.query(alertQueries.integrations()).catch(noop);
    void queryClient.query(githubQueries.status()).catch(noop);
  },
  component: IntegrationsPage,
});

/**
 * What Rustrak connects to, as a catalogue: one card per integration, each
 * leading to its own settings page. Nothing is configured here.
 */
function IntegrationsPage() {
  const t = useTranslations('settings');
  const tAlerts = useTranslations('alerts');
  const tGitHub = useTranslations('github');
  const integrations = useQuery(alertQueries.integrations());
  const github = useQuery(githubQueries.status());
  const { projectId } = Route.useSearch();

  const configured = (type: string) =>
    (integrations.data ?? []).filter((i) => i.provider_type === type);

  const githubInstalled = (github.data?.installations.length ?? 0) > 0;
  const githubPending = !!github.data?.app && !githubInstalled;

  return (
    <>
      <title>{t('integrations.meta.title')}</title>
      <SettingsHeader
        title={t('integrations.title')}
        description={t('integrations.subtitle')}
        actions={
          // Arriving from a project's alert rules, the way back is one click.
          projectId ? (
            <Button
              variant="secondary"
              render={<Link href={integrationsBackHref(projectId)} />}
            >
              {t('integrations.backToAlerts')}
            </Button>
          ) : null
        }
      />

      {/* A failed read is said, not shown as nothing configured. */}
      {integrations.isError ? (
        <LoadFailure
          error={integrations.error.error}
          title={t('integrations.loadFailed')}
          notFoundOnMissing={false}
        />
      ) : null}
      {github.isError ? (
        <LoadFailure
          error={github.error.error}
          title={t('integrations.githubLoadFailed')}
          notFoundOnMissing={false}
        />
      ) : null}

      <div className="grid grid-cols-1 gap-3.5 lg:grid-cols-2 2xl:grid-cols-3">
        <IntegrationCard
          href="/settings/integrations/github"
          icon={<GitHubMark className="size-5" />}
          name="GitHub"
          description={tGitHub('connection.description')}
          status={
            githubInstalled
              ? tAlerts('integrations.connected')
              : githubPending
                ? tGitHub('connection.notInstalled')
                : null
          }
          tone={githubInstalled ? 'brand' : 'warning'}
        />
        {alertProviders.map((provider) => {
          const channels = configured(provider.type);
          return (
            <IntegrationCard
              key={provider.type}
              href={`/settings/integrations/${provider.type}`}
              icon={<ProviderIcon type={provider.type} className="size-5" />}
              name={provider.name}
              description={tAlerts(
                provider.descriptionKey as 'providers.slack.description',
              )}
              status={
                channels.length > 0
                  ? tAlerts('integrations.configuredCount', {
                      count: channels.length,
                    })
                  : null
              }
              tone="brand"
            />
          );
        })}
      </div>
    </>
  );
}

/** One integration: its mark, whether it is set up, and the way in. */
function IntegrationCard({
  href,
  icon,
  name,
  description,
  status,
  tone,
}: {
  href: string;
  icon: ReactNode;
  name: string;
  description: string;
  /** Null when it is not set up: no badge, the card says what it does. */
  status: string | null;
  tone: TagTone;
}) {
  return (
    <Link
      href={href}
      className={cn(
        'flex flex-col gap-3 rounded-2xl border border-border-subtle bg-surface p-5 outline-none',
        'transition-[border-color] duration-instant ease-standard',
        'hover:border-border-strong focus-visible:ring-2 focus-visible:ring-ring',
      )}
    >
      <div className="flex items-center gap-3">
        <span className="flex size-10 shrink-0 items-center justify-center rounded-xl border border-border-subtle text-fg-secondary">
          {icon}
        </span>
        <span className="min-w-0 flex-1 truncate text-section text-fg">
          {name}
        </span>
        {status ? (
          <Tag variant="soft" tone={tone}>
            {status}
          </Tag>
        ) : null}
      </div>
      <p className="line-clamp-2 text-meta text-fg-subtle">{description}</p>
    </Link>
  );
}
