import { createFileRoute, redirect } from '@tanstack/react-router';
import { Rocket } from 'lucide-react';
import { useTranslations } from 'use-intl';
import { getProject } from '@/features/project/api/queries';
import { getReleaseHealth } from '@/features/release/api/queries';
import { parseReleasePeriod } from '@/features/release/model/session-health';
import { ReleasesList } from '@/features/release/ui/components/releases-list';
import { translator } from '@/shared/i18n/intl';
import { searchPage, searchString } from '@/shared/lib/search-params';
import { LoadFailure } from '@/shared/ui/components/load-failure';

/** Canonical URL for a page of the releases list, keeping the active window. */
function releasesHref(
  projectId: number,
  page: number,
  period?: string,
  environment?: string,
): string {
  const params = new URLSearchParams({ page: String(page) });
  if (period) params.set('period', period);
  if (environment) params.set('environment', environment);
  return `/projects/${projectId}/releases?${params.toString()}`;
}

export const Route = createFileRoute('/_authenticated/projects/$id/releases/')({
  validateSearch: (search: Record<string, unknown>) => ({
    page: searchPage(search.page),
    // An unrecognized window would otherwise reach the API, which ignores what
    // it cannot parse and answers with all-time data while no filter button
    // reads as selected. Drop it instead, so the URL and the UI always agree.
    period: parseReleasePeriod(searchString(search.period)),
    environment: searchString(search.environment),
  }),
  loaderDeps: ({ search }) => ({
    page: search.page ?? 1,
    period: search.period,
    environment: search.environment,
  }),
  loader: async ({ params, deps }) => {
    const projectId = Number.parseInt(params.id, 10);
    const project = await getProject(projectId);

    if (!project.success) return { project, health: null };

    // Nothing is swallowed: a fetch/auth failure renders an outage surface
    // rather than the "no releases yet" onboarding state.
    const health = await getReleaseHealth(projectId, {
      page: deps.page,
      per_page: 20,
      period: deps.period,
      environment: deps.environment,
    });

    // A page past the end still carries a positive total, which would render a
    // nonsensical range ("19961-27 of 27", "Page 999 of 2"). Send the browser
    // to a page that exists; the target is always within range, so this settles
    // in one hop.
    if (health.success) {
      const { total_pages } = health.data;
      if (total_pages > 0 && deps.page > total_pages) {
        throw redirect({
          href: releasesHref(
            projectId,
            total_pages,
            deps.period,
            deps.environment,
          ),
        });
      }
      if (total_pages === 0 && deps.page > 1) {
        throw redirect({
          href: releasesHref(projectId, 1, deps.period, deps.environment),
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
  const { id } = Route.useParams();
  const { page, period, environment } = Route.useSearch({
    select: (search) => ({
      page: search.page ?? 1,
      period: search.period,
      environment: search.environment,
    }),
  });
  const { project: projectResult, health: healthResult } =
    Route.useLoaderData();
  const projectId = Number.parseInt(id, 10);

  if (!projectResult.success) {
    return (
      <LoadFailure error={projectResult.error} title={t('loadProjectFailed')} />
    );
  }

  // `null` only where the project read already failed, and that branch
  // returned above — so this one is a real failure of its own request.
  if (healthResult === null) return null;

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
    <div className="flex flex-col h-full">
      <div className="shrink-0 w-full px-4 md:px-8 py-4 md:py-6 border-b">
        <h1 className="text-lg font-semibold">{t('releases.title')}</h1>
        <p className="text-sm text-muted-foreground mt-0.5">
          {t('releases.subtitle', { project: project.name })}
        </p>
      </div>

      <div className="flex-1 overflow-hidden w-full px-4 md:px-8 py-4 md:py-6">
        {health.total_count === 0 && !period && !environment ? (
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
