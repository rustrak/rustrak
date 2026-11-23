import type {
  ProjectResponse,
  WebVitalsListPagesResponse,
  WebVitalsSummary,
} from '@rustrak/client';
import { Page, PageHeader, Skeleton } from '@rustrak/ui';
import { noop, useQuery, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useDeferredValue } from 'react';
import { useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import { webVitalQueries } from '@/features/web-vital/api/queries';
import {
  firstFix,
  metricHref,
  pageHref,
} from '@/features/web-vital/model/web-vitals';
import {
  FirstFix,
  PagesTable,
  Verdict,
} from '@/features/web-vital/ui/components/web-vitals';
import { type TableSearch, tableSearch } from '@/shared/lib/table-search';
import { EmptyState } from '@/shared/ui/components/empty-state';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { PanelPlaceholder } from '@/shared/ui/components/panel';
import { useRouter } from '@/shared/ui/hooks/use-router';
import { MetricCards } from './-components/metric-cards';
import {
  apiScope,
  scopeChange,
  validateScope,
  type WebVitalsScope,
} from './-components/scope';
import { ScopeActions } from './-components/web-vitals-filters';

const PAGE_SIZE = 25;

/** How many of the costliest pages "where to start" looks through. */
const WHERE_TO_START = 25;

/** Only pagination: the server ranks the pages, by opportunity. */
const table = tableSearch({
  variants: {},
  pageSize: PAGE_SIZE,
  normalise: (query) => ({ ...query, sorting: [], filters: [], search: '' }),
});

type WebVitalsSearch = TableSearch & WebVitalsScope;

/** The reads of one address, for the loader and the components alike. */
function reads(projectId: number, search: WebVitalsSearch) {
  const scope = apiScope(search);
  const { pagination } = table.read(search);
  return {
    summary: webVitalQueries.summary(projectId, scope),
    pages: webVitalQueries.pages(projectId, {
      ...scope,
      page: pagination.pageIndex + 1,
      per_page: pagination.pageSize,
    }),
    // The costliest pages, whichever page of the table is open.
    top: webVitalQueries.pages(projectId, {
      ...scope,
      per_page: WHERE_TO_START,
    }),
  };
}

export const Route = createFileRoute(
  '/_authenticated/projects/$id/web-vitals/',
)({
  validateSearch: (search: Record<string, unknown>): WebVitalsSearch => ({
    ...table.validate(search),
    ...validateScope(search),
  }),
  loaderDeps: ({ search }) => search,
  loader: ({ context: { queryClient }, params, deps }) => {
    const r = reads(params.id, deps);
    void queryClient.query(r.summary).catch(noop);
    void queryClient.query(r.pages).catch(noop);
    void queryClient.query(r.top).catch(noop);
  },
  pendingComponent: WebVitalsLoading,
  component: WebVitalsPage,
});

/**
 * The verdict and the pages are what the page waits for; where to start
 * lands on its own. A new filter keeps what is on screen, dimmed, until it
 * is in.
 */
function WebVitalsPage() {
  const { id: projectId } = Route.useParams();
  const live = Route.useSearch();
  const search = useDeferredValue(live);
  const r = reads(projectId, search);
  const { data: project } = useSuspenseQuery(projectQueries.detail(projectId));
  const { data: summary } = useSuspenseQuery(r.summary);
  const { data: pages } = useSuspenseQuery(r.pages);
  return (
    <WebVitalsScreen
      project={project}
      summary={summary}
      pages={pages}
      loading={search !== live}
    />
  );
}

function WebVitalsLoading() {
  return (
    <PageLoading>
      <WebVitalsScreen loading={false} />
    </PageLoading>
  );
}

function WebVitalsScreen({
  project,
  summary,
  pages,
  loading,
}: {
  project?: ProjectResponse;
  summary?: WebVitalsSummary;
  pages?: WebVitalsListPagesResponse;
  loading: boolean;
}) {
  const t = useTranslations('webVitals');
  const { id: projectId } = Route.useParams();
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const router = useRouter();
  const scope: WebVitalsScope = {
    period: search.period,
    environment: search.environment,
    browser: search.browser,
    device: search.device,
  };
  const empty =
    summary?.vitals.length === 0 &&
    search.environment === undefined &&
    search.browser === undefined &&
    search.device === undefined;

  return (
    <div className="flex h-full flex-col max-lg:overflow-y-auto">
      {project ? (
        <title>{t('meta.title', { project: project.name })}</title>
      ) : null}
      <PageHeader
        title={t('title')}
        summary={
          project && pages
            ? [project.name, t('pages', { count: pages.total_count })]
            : [<Skeleton key="summary" className="h-4 w-56" />]
        }
        actions={
          <ScopeActions
            projectId={projectId}
            scope={scope}
            browsers={summary?.browsers ?? []}
            held={!summary}
            onChange={(next) =>
              navigate({
                search: (prev) => ({
                  ...prev,
                  ...scopeChange(next),
                  page: undefined,
                }),
              })
            }
          />
        }
      />

      {empty ? (
        <NoWebVitals />
      ) : (
        <Page className="max-lg:flex-none max-lg:overflow-visible">
          {summary ? (
            <>
              <Verdict
                summary={summary}
                aside={<WhereToStart projectId={projectId} scope={scope} />}
              />
              <MetricCards
                projectId={projectId}
                summary={summary}
                scope={scope}
              />
            </>
          ) : (
            <PanelPlaceholder height={420} />
          )}
          <section className="flex min-h-96 flex-col gap-3">
            <h2 className="text-fg text-section">{t('pagesTitle')}</h2>
            <PagesTable
              page={pages}
              query={table.read(search)}
              loading={loading}
              onOpen={(row) =>
                router.push(pageHref(projectId, row.page, scope))
              }
              onQueryChange={(updater) =>
                navigate({
                  search: (prev) => ({
                    ...prev,
                    ...table.apply(updater, prev),
                  }),
                })
              }
            />
          </section>
        </Page>
      )}
    </div>
  );
}

/**
 * The costliest page with a vital below good, its worst vital and the
 * element behind it, linking to that vital on that page. When none of the
 * costliest pages has one, says so, of those pages only.
 */
function WhereToStart({
  projectId,
  scope,
}: {
  projectId: number;
  scope: WebVitalsScope;
}) {
  const tFix = useTranslations('webVitals.firstFix');
  const top = useQuery(
    webVitalQueries.pages(projectId, {
      ...apiScope(scope),
      per_page: WHERE_TO_START,
    }),
  );
  const fix = top.data ? firstFix(top.data.items) : null;
  const causes = useQuery({
    ...webVitalQueries.targets(projectId, {
      ...apiScope(scope),
      page: fix?.page,
      vital: fix?.vital,
    }),
    enabled: !!fix,
  });
  if (!fix) {
    return top.data ? (
      <p className="flex items-center px-5 py-4 text-fg-subtle text-meta">
        {tFix('none', { count: top.data.items.length })}
      </p>
    ) : null;
  }
  return (
    <FirstFix
      {...fix}
      cause={causes.data?.[0]?.target}
      href={metricHref(projectId, fix.vital, { ...scope, page: fix.page })}
    />
  );
}

/** Before any vital arrives: what the browser SDK needs. */
function NoWebVitals() {
  const t = useTranslations('webVitals');
  return (
    <EmptyState
      title={t('empty.title')}
      description={t.rich('empty.description', {
        code: (chunks) => <code className="font-mono">{chunks}</code>,
      })}
    />
  );
}
