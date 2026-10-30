import type { IssuesListResponse, ProjectResponse } from '@rustrak/client';
import {
  type DataTableQuery,
  Page,
  PageHeader,
  parseTableQuery,
  Skeleton,
  serializeTableQuery,
  Tab,
  TabList,
  Tabs,
} from '@rustrak/ui';
import { noop, useQuery, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useDeferredValue } from 'react';
import { useFormatter, useTranslations } from 'use-intl';
import { issueQueries } from '@/features/issue/api/queries';
import { assignable } from '@/features/issue/model/assignable';
import {
  ISSUE_FILTER_VARIANTS,
  ISSUE_VIEWS,
  type IssueView,
  issueListParams,
  withServerQuery,
} from '@/features/issue/model/issue-table';
import { IssuesTable } from '@/features/issue/ui/components/issues-table/issues-table';
import { projectQueries } from '@/features/project/api/queries';
import { userQueries } from '@/features/user/api/queries';
import { searchOneOf, searchString } from '@/shared/lib/search-params';
import { PageLoading } from '@/shared/ui/components/page-loading';

const PAGE_SIZE = 50;

/**
 * The search, exactly as the table's codecs write it: `q` holds the filters
 * and the free text in the query bar's own syntax, so the address is the bar.
 */
interface IssuesSearch {
  view?: IssueView;
  q?: string;
  sort?: string;
  page?: string;
  per?: string;
}

function tableQuery(search: IssuesSearch): DataTableQuery {
  const params = new URLSearchParams();
  for (const key of ['q', 'sort', 'page', 'per'] as const) {
    const value = search[key];
    if (value) params.set(key, value);
  }
  return parseTableQuery(params, ISSUE_FILTER_VARIANTS, PAGE_SIZE);
}

/** The page of issues a URL shows, for the loader and the component alike. */
function issuesList(projectId: number, search: IssuesSearch) {
  return issueQueries.list(
    projectId,
    issueListParams(tableQuery(search), search.view ?? 'open'),
  );
}

/** A query and a view, written the way the address holds them. */
function toSearch(query: DataTableQuery, view: IssueView): IssuesSearch {
  const params = serializeTableQuery(query, ISSUE_FILTER_VARIANTS, PAGE_SIZE);
  return {
    view: view === 'open' ? undefined : view,
    q: params.get('q') ?? undefined,
    sort: params.get('sort') ?? undefined,
    page: params.get('page') ?? undefined,
    per: params.get('per') ?? undefined,
  };
}

export const Route = createFileRoute('/_authenticated/projects/$id/issues/')({
  // Read and written back, so the address keeps only what the table reads:
  // an unknown sort, or a page that is not a number, is dropped rather than
  // left in a URL that no control shows.
  validateSearch: (search: Record<string, unknown>): IssuesSearch => {
    const view =
      searchOneOf(search.view, ISSUE_VIEWS) ??
      // What the list's address called it before the redesign: links saved
      // then still open the view they name.
      searchOneOf(search.filter, ISSUE_VIEWS) ??
      'open';
    const raw = (value: unknown) =>
      searchString(value == null ? undefined : String(value));
    const query = tableQuery({
      q: raw(search.q),
      sort: raw(search.sort),
      page: raw(search.page),
      per: raw(search.per),
    });
    return toSearch(withServerQuery(query, PAGE_SIZE), view);
  },
  loaderDeps: ({ search }) => search,
  loader: ({ context: { queryClient }, params, deps }) => {
    const projectId = Number.parseInt(params.id, 10);
    void queryClient.query(projectQueries.detail(projectId)).catch(noop);
    void queryClient.query(issuesList(projectId, deps)).catch(noop);
    void queryClient.query(userQueries.members(projectId)).catch(noop);
    void queryClient.query(projectQueries.environments(projectId)).catch(noop);
    void queryClient
      .query(projectQueries.releaseVersions(projectId))
      .catch(noop);
  },
  pendingComponent: IssuesLoading,
  component: IssuesPage,
});

/**
 * The list and the project it belongs to are what the page waits for. The
 * page itself follows the address a beat behind: a new filter or page keeps
 * the rows on screen, dimmed, until the next page is in, instead of
 * swapping the whole list for a skeleton on every keystroke.
 */
function IssuesPage() {
  const { id } = Route.useParams();
  const projectId = Number.parseInt(id, 10);
  const live = Route.useSearch();
  const search = useDeferredValue(live);
  const { data: project } = useSuspenseQuery(projectQueries.detail(projectId));
  const { data: page } = useSuspenseQuery(issuesList(projectId, search));

  return (
    <IssuesScreen project={project} page={page} loading={search !== live} />
  );
}

/** The same screen, with the project's facts and the rows still to come. */
function IssuesLoading() {
  return (
    <PageLoading>
      <IssuesScreen loading />
    </PageLoading>
  );
}

function IssuesScreen({
  project,
  page,
  loading,
}: {
  project?: ProjectResponse;
  page?: IssuesListResponse;
  loading: boolean;
}) {
  const t = useTranslations('projectPages');
  const tIssues = useTranslations('issues');
  const format = useFormatter();
  const { id } = Route.useParams();
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const context = Route.useRouteContext();
  const projectId = Number.parseInt(id, 10);
  const me = context.state === 'authenticated' ? context.user.id : 0;
  const view = search.view ?? 'open';
  const query = tableQuery(search);
  // Suggestions only: without them the list still works.
  const members = useQuery(userQueries.members(projectId)).data ?? [];
  const environments =
    useQuery(projectQueries.environments(projectId)).data ?? [];
  const releases =
    useQuery(projectQueries.releaseVersions(projectId)).data ?? [];

  // Each change applies to the address as it is when it lands, not as it was
  // when this render read it: two changes in one gesture (clearing the
  // filters and then the search) must not have the second undo the first.
  const setQuery = (
    update: (previous: DataTableQuery) => DataTableQuery,
    nextView?: IssueView,
  ) =>
    navigate({
      search: (previous) =>
        toSearch(
          update(tableQuery(previous)),
          nextView ?? previous.view ?? 'open',
        ),
    });

  return (
    <div className="flex h-full flex-col">
      {project ? (
        <title>{t('projectTitle', { project: project.name })}</title>
      ) : null}
      <PageHeader
        title={t('issues.title')}
        // What the page is, then how much of it there is.
        summary={
          project && page
            ? [
                t('issues.subtitle', { project: project.name }),
                tIssues('table.count', {
                  count: page.total_count,
                  display: format.number(page.total_count),
                }),
              ]
            : [<Skeleton key="summary" className="h-4 w-56" />]
        }
        tabs={
          <Tabs
            value={view}
            onValueChange={(next) =>
              setQuery(
                (previous) => ({
                  ...previous,
                  pagination: { ...previous.pagination, pageIndex: 0 },
                }),
                next as IssueView,
              )
            }
          >
            <TabList>
              {ISSUE_VIEWS.map((value) => (
                <Tab key={value} value={value}>
                  {tIssues(`table.views.${value}`)}
                </Tab>
              ))}
            </TabList>
          </Tabs>
        }
      />

      <Page scroll={false}>
        <IssuesTable
          projectId={projectId}
          page={page}
          query={query}
          view={view}
          onQueryChange={setQuery}
          loading={loading}
          me={me}
          members={assignable(
            members,
            context.state === 'authenticated' ? context.user : null,
          )}
          environments={environments}
          releases={releases}
        />
      </Page>
    </div>
  );
}
