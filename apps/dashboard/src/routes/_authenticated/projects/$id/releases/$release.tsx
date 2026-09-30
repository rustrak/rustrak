import { createFileRoute, notFound } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { IssueListCard } from '@/features/issue/ui/components/issue-list-card';
import { getProject } from '@/features/project/api/queries';
import {
  getAllReleaseHealthRows,
  getNewIssuesForRelease,
} from '@/features/release/api/queries';
import { ReleaseEnvironmentCards } from '@/features/release/ui/components/release-environment-cards';
import { translator } from '@/shared/i18n/intl';
import { loadAll } from '@/shared/lib/results';
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
  validateSearch: (search: Record<string, unknown>) => ({
    environment: searchString(search.environment),
  }),
  loaderDeps: ({ search }) => ({ environment: search.environment }),
  loader: async ({ params, deps }) => {
    const projectId = Number.parseInt(params.id, 10);
    const releaseVersion = decodeRelease(params.release);

    // A malformed escape is not a release this instance has ever seen, so it
    // is the same answer as one that was never reported. `decodeURIComponent`
    // throws `URIError` on `%zz`, and letting that reach the router turns a
    // wrong address into the application-error screen.
    if (releaseVersion === null) throw notFound();

    // Started here, awaited below. It is kept *out* of `loadAll` because the
    // page does not need it: the environment cards are the release's real
    // content and stand on their own, so its failure degrades this one panel
    // instead of the page -- and it degrades to a failure, never to "no new
    // issues introduced in this release", which is a statement about the
    // release we did not obtain.
    //
    // Kept out of `loadAll` but not out of the same round-trip: awaiting it
    // after `loadAll` resolved would isolate the failure and serialise the
    // request, when only the first was wanted.
    const newIssuesPromise = getNewIssuesForRelease(
      projectId,
      releaseVersion,
      10,
      deps.environment,
    );

    const loaded = await loadAll([
      getProject(projectId),
      getAllReleaseHealthRows(
        projectId,
        releaseVersion,
        undefined,
        deps.environment,
      ),
    ]);

    // Both early exits drain the in-flight request above, so a transport-level
    // failure on a promise nothing is waiting for cannot surface as an
    // unhandled rejection.
    if (!loaded.success) {
      void newIssuesPromise.catch(() => undefined);
      return { loaded, newIssues: null, releaseVersion };
    }

    // No health rows at all means the release in the URL was never reported.
    // Distinct from the failure above, which is why the check stays after it —
    // and before the second await, so a wrong address does not wait on a
    // request whose answer it will not use.
    if (loaded.data[1].length === 0) {
      const allRows = deps.environment
        ? await getAllReleaseHealthRows(projectId, releaseVersion)
        : null;
      if (allRows && !allRows.success) {
        void newIssuesPromise.catch(() => undefined);
        return {
          loaded: allRows,
          newIssues: null,
          releaseVersion,
        };
      }
      if (!allRows || allRows.data.length === 0) {
        void newIssuesPromise.catch(() => undefined);
        throw notFound();
      }
    }

    const newIssues = await newIssuesPromise;

    return { loaded, newIssues, releaseVersion };
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
  const { id } = Route.useParams();
  const { environment } = Route.useSearch();
  const { loaded, newIssues, releaseVersion } = Route.useLoaderData();
  const projectId = Number.parseInt(id, 10);

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
    <div className="flex flex-col h-full overflow-auto">
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
          newIssues && (
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
          )
        )}
      </div>
    </div>
  );
}
