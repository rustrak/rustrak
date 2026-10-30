import { Skeleton } from '@rustrak/ui';
import { noop, useQuery, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import { RateLimitSettings } from '@/features/project/ui/components/general-settings-form/rate-limit-settings';
import { ProductsSettings } from '@/features/project/ui/components/products-settings';
import { PageLoading } from '@/shared/ui/components/page-loading';
import {
  SettingRow,
  SettingSection,
  SettingsListSkeleton,
} from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';

export const Route = createFileRoute(
  '/_authenticated/projects/$id/settings/data',
)({
  // The project is the layout's; the server's limits are only a placeholder.
  loader: ({ context: { queryClient } }) => {
    void queryClient.query(projectQueries.rateLimits()).catch(noop);
  },
  pendingComponent: DataSettingsLoading,
  component: DataSettingsPage,
});

/**
 * The project's control over its ingest: which kinds of data it keeps, and
 * how much of it per minute and hour. Both answer "what lands in this
 * project", so they are one page.
 */
function DataSettingsPage() {
  const t = useTranslations('settings');
  const { id } = Route.useParams();
  const { data: project } = useSuspenseQuery(
    projectQueries.detail(Number.parseInt(id, 10)),
  );
  const rateLimits = useQuery(projectQueries.rateLimits());

  return (
    <>
      <title>{`${t('data.title')} | Rustrak`}</title>
      <SettingsHeader
        title={t('data.title')}
        description={t('data.subtitle')}
      />

      {/* Keyed: its state seeds once, and the next project must not inherit
          this one's switches. */}
      <ProductsSettings key={project.id} project={project} />

      <RateLimitSettings
        project={project}
        // A failure here costs only the placeholder, not the page.
        serverLimits={rateLimits.data ?? null}
      />
    </>
  );
}

function DataSettingsLoading() {
  const t = useTranslations('settings');
  const tLimits = useTranslations('projects.rateLimits');
  return (
    <PageLoading>
      <title>{`${t('data.title')} | Rustrak`}</title>
      <SettingsHeader
        title={t('data.title')}
        description={t('data.subtitle')}
      />
      <SettingSection
        title={t('products.title')}
        description={t('products.note')}
      >
        <SettingsListSkeleton />
      </SettingSection>
      <SettingSection
        title={tLimits('title')}
        description={tLimits('description')}
      >
        {[tLimits('perMinute'), tLimits('perHour')].map((title) => (
          <SettingRow
            key={title}
            title={title}
            description={tLimits('limitDescription')}
          >
            <Skeleton className="h-9 w-full rounded-lg" />
          </SettingRow>
        ))}
      </SettingSection>
    </PageLoading>
  );
}
