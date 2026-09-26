import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, notFound } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { IssueListCard } from '@/features/issue/ui/components/issue-list-card';
import { projectQueries } from '@/features/project/api/queries';
import { releaseQueries } from '@/features/release/api/queries';
import { ReleaseEnvironmentCards } from '@/features/release/ui/components/release-environment-cards';
import { translator } from '@/shared/i18n/intl';
import { combine, loadAll } from '@/shared/lib/results';
import { searchString } from '@/shared/lib/search-params';
import { LoadFailure } from '@/shared/ui/components/load-failure';
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from '@/shared/ui/components/shadcn/card';

/** The release from the path, or `null` if it is not a decodable one. */
function decodeRelease(raw: string): string | null {
  try {
    return decodeURIComponent(raw);
  } catch {
    return null;
  }
}

export const Route = createFileRoute(
  '/_authenticated/projects/$id/releases/$release',
)({
  validateSearch: (
    search: Record<string, unknown>,
  ): { environment?: string } => ({
    environment: searchString(search.environment),
  }),
  loader: async ({ params, context: { queryClient } }) => {
    const releaseVersion = decodeRelease(params.release);

    // A malformed escape is not a release this instance has ever seen, so it
    // is the same answer as one that was never reported. `decodeURIComponent`
    // throws `URIError` on `%zz`, and letting that reach the router turns a
    // wrong address into the application-error screen.
    if (releaseVersion === null) throw notFound();

    // The new issues are kept *out* of `loadAll` because the page does not
    // need them: the environment cards are the release's real content and
    // stand on their own, so their failure degrades this one panel instead of
    // the page. Same round trip all the same.
    const [loaded] = await Promise.all([
      loadAll([
        queryClient.ensureQueryData(projectQueries.detail(params.id)),
        queryClient.ensureQueryData(
          releaseQueries.rows(params.id, releaseVersion),
        ),
      ]),
      queryClient.ensureQueryData(
        releaseQueries.newIssues(params.id, releaseVersion, 10),
      ),
    ]);

    // No health rows at all means the release in the URL was never reported.
    if (loaded.success && loaded.data[1].length === 0) throw notFound();

    return { loaded, releaseVersion };
  },
  head: ({ loaderData }) => {
    const t = translator('projectPages');

    if (!loaderData?.loaded.success) {
      return { meta: [{ title: t('projectNotFound') }] };
    }

    const [project] = loaderData.loaded.data;
    return {
      meta: [
        {
          title: t('releaseDetail.meta.title', {
            release: loaderData.releaseVersion,
            project: project.name,
          }),
        },
        {
          name: 'description',
          content: t('releaseDetail.meta.description', {
            release: loaderData.releaseVersion,
          }),
        },
      ],
    };
  },
  component: ReleaseDetailPage,
});

function ReleaseDetailPage() {
  const t = useTranslations('projectPages');
  const { id: projectId } = Route.useParams();
  const { environment } = Route.useSearch();
  const { releaseVersion } = Route.useLoaderData();
  const loaded = combine([
    useSuspenseQuery(projectQueries.detail(projectId)).data,
    useSuspenseQuery(releaseQueries.rows(projectId, releaseVersion)).data,
  ]);
  const newIssues = useSuspenseQuery(
    releaseQueries.newIssues(projectId, releaseVersion, 10),
  ).data;

  if (!loaded.success) {
    return (
      <LoadFailure error={loaded.error} title={t('releaseDetail.loadFailed')} />
    );
  }

  const [project, rows] = loaded.data;

  const visibleRows = environment
    ? rows.filter((row) => row.environment === environment)
    : rows;

  return (
    <div className="flex flex-col h-[calc(100vh-64px)] overflow-auto">
      <div className="shrink-0 w-full px-4 md:px-8 py-4 md:py-6 border-b">
        <h1 className="text-lg font-semibold font-mono">{releaseVersion}</h1>
        <p className="text-sm text-muted-foreground mt-0.5">
          {t('releaseDetail.subtitle', { project: project.name })}
        </p>
      </div>

      <div className="flex-1 w-full px-4 md:px-8 py-4 md:py-6 flex flex-col gap-4">
        <ReleaseEnvironmentCards rows={visibleRows} />

        {newIssues?.success ? (
          <IssueListCard
            projectId={projectId}
            issues={newIssues.data}
            title={t('releaseDetail.newIssues')}
            emptyMessage={t('releaseDetail.newIssuesEmpty')}
          />
        ) : (
          <Card size="sm">
            <CardHeader>
              <CardTitle className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
                {t('releaseDetail.newIssues')}
              </CardTitle>
            </CardHeader>
            <CardContent>
              {/* A 404 from this endpoint alone is not grounds for replacing
                    a release page that already rendered its health cards. */}
              <LoadFailure
                error={newIssues.error}
                title={t('releaseDetail.loadNewIssuesFailed')}
                notFoundOnMissing={false}
              />
            </CardContent>
          </Card>
        )}
      </div>
    </div>
  );
}
