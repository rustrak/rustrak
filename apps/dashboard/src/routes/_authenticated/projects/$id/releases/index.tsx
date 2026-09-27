import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, redirect } from '@tanstack/react-router';
import { Rocket } from 'lucide-react';
import { useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import { releaseQueries } from '@/features/release/api/queries';
import {
  parseReleasePeriod,
  type ReleasePeriod,
} from '@/features/release/model/session-health';
import { ReleasesList } from '@/features/release/ui/components/releases-list';
import { translator } from '@/shared/i18n/intl';
import { searchPage, searchString } from '@/shared/lib/search-params';
import { LoadFailure } from '@/shared/ui/components/load-failure';

function healthQuery(projectId: number, page: number, period?: string) {
  return releaseQueries.health(projectId, { page, per_page: 20, period });
}

export const Route = createFileRoute('/_authenticated/projects/$id/releases/')({
  validateSearch: (
    search: Record<string, unknown>,
  ): {
    page?: number;
    period?: ReleasePeriod;
  } => ({
    page: searchPage(search.page),
    // An unrecognized window would otherwise reach the API, which ignores what
    // it cannot parse and answers with all-time data while no filter button
    // reads as selected. Drop it instead, so the URL and the UI always agree.
    period: parseReleasePeriod(searchString(search.period)),
  }),
  loaderDeps: ({ search }) => ({
    page: search.page ?? 1,
    period: search.period,
  }),
  loader: async ({ params: { id }, deps, context: { queryClient } }) => {
    // Nothing is swallowed: a fetch/auth failure renders an outage surface
    // rather than the "no releases yet" onboarding state.
    const [project, health] = await Promise.all([
      queryClient.ensureQueryData(projectQueries.detail(id)),
      queryClient.ensureQueryData(healthQuery(id, deps.page, deps.period)),
    ]);

    // A page past the end still carries a positive total, which would render a
    // nonsensical range ("19961-27 of 27", "Page 999 of 2"). Send the browser
    // to a page that exists; the target is always within range, so this settles
    // in one hop.
    if (health.success) {
      const { total_pages } = health.data;
      const last = Math.max(total_pages, 1);
      if (deps.page > last) {
        throw redirect({
          to: '/projects/$id/releases',
          params: { id },
          search: { page: last, period: deps.period },
        });
      }
    }

    return { project, health };
  },
  head: ({ loaderData }) => {
    const t = translator('projectPages');

    if (!loaderData?.project.success) {
      return { meta: [{ title: t('projectNotFound') }] };
    }

    const project = loaderData.project.data;
    return {
      meta: [
        { title: t('releases.meta.title', { project: project.name }) },
        {
          name: 'description',
          content: t('releases.meta.description', { project: project.name }),
        },
      ],
    };
  },
  component: ReleasesPage,
});

function ReleasesPage() {
  const t = useTranslations('projectPages');
  const { id: projectId } = Route.useParams();
  const { page, period } = Route.useSearch({
    select: (search) => ({ page: search.page ?? 1, period: search.period }),
  });
  const projectResult = useSuspenseQuery(projectQueries.detail(projectId)).data;
  const healthResult = useSuspenseQuery(
    healthQuery(projectId, page, period),
  ).data;

  if (!projectResult.success) {
    return (
      <LoadFailure error={projectResult.error} title={t('loadProjectFailed')} />
    );
  }

  if (!healthResult.success) {
    return (
      <LoadFailure
        error={healthResult.error}
        title={t('releases.loadFailed')}
        notFoundOnMissing={false}
      />
    );
  }

  const project = projectResult.data;
  const health = healthResult.data;

  return (
    <div className="flex flex-col h-[calc(100vh-64px)]">
      <div className="shrink-0 w-full px-4 md:px-8 py-4 md:py-6 border-b">
        <h1 className="text-lg font-semibold">{t('releases.title')}</h1>
        <p className="text-sm text-muted-foreground mt-0.5">
          {t('releases.subtitle', { project: project.name })}
        </p>
      </div>

      <div className="flex-1 overflow-hidden w-full px-4 md:px-8 py-4 md:py-6">
        {health.total_count === 0 && !period ? (
          <div className="flex flex-col items-center justify-center min-h-full text-center">
            <Rocket className="size-12 text-muted-foreground/30 mb-4" />
            <h2 className="text-lg font-semibold mb-1">
              {t('releases.emptyTitle')}
            </h2>
            <p className="text-sm text-muted-foreground max-w-md">
              {t.rich('releases.emptyDescription', {
                code: (chunks) => <code>{chunks}</code>,
              })}
            </p>
          </div>
        ) : (
          <ReleasesList
            projectId={projectId}
            initialHealth={health}
            currentPage={page}
            activePeriod={period}
          />
        )}
      </div>
    </div>
  );
}
