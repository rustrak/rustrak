import type { IssuesListResponse, ProjectResponse } from '@rustrak/client';
import {
  type DataTableQuery,
  Page,
  PageHeader,
  Skeleton,
  Tab,
  TabList,
  Tabs,
} from '@rustrak/ui';
import { noop, useQuery, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, Navigate } from '@tanstack/react-router';
import { useDeferredValue } from 'react';
import { useFormatter, useTranslations } from 'use-intl';
import { issueQueries } from '@/features/issue/api/queries';
import {
  ISSUE_FILTER_VARIANTS,
  ISSUE_VIEWS,
  type IssueView,
  issueListParams,
  withServerQuery,
} from '@/features/issue/model/issue-table';
import { IssuesTable } from '@/features/issue/ui/components/issues-table/issues-table';
import { useAssignable } from '@/features/issue/ui/hooks/use-assignable';
import { projectQueries } from '@/features/project/api/queries';
import { savedViewQueries } from '@/features/saved-view/api/queries';
import { SavedViews } from '@/features/saved-view/ui/components/saved-views';
import { useEntryView } from '@/features/saved-view/ui/hooks/use-entry-view';
import { userQueries } from '@/features/user/api/queries';
import { startIfAllowed } from '@/shared/api/permissions';
import { searchOneOf } from '@/shared/lib/search-params';
import {
  pageBeyondEnd,
  type TableSearch,
  tableSearch,
} from '@/shared/lib/table-search';
import { PageLoading } from '@/shared/ui/components/page-loading';

const PAGE_SIZE = 50;

/**
 * The search, exactly as the table's codecs write it: `q` holds the filters
 * and the free text in the query bar's own syntax, so the address is the bar.
 */
interface IssuesSearch extends TableSearch {
  view?: IssueView;
}

const table = tableSearch({
  variants: ISSUE_FILTER_VARIANTS,
  pageSize: PAGE_SIZE,
  normalise: withServerQuery,
});

/** The page of issues a URL shows, for the loader and the component alike. */
function issuesList(projectId: number, search: IssuesSearch) {
  return issueQueries.list(
    projectId,
    issueListParams(table.read(search), search.view ?? 'open'),
  );
}

function toSearch(query: DataTableQuery, view: IssueView): IssuesSearch {
  return { view: view === 'open' ? undefined : view, ...table.write(query) };
}

/** The address as typed search; a saved view's text goes through it too. */
function validateIssuesSearch(search: Record<string, unknown>): IssuesSearch {
  const view =
    searchOneOf(search.view, ISSUE_VIEWS) ??
    // What the list's address called it before the redesign: links saved
    // then still open the view they name.
    searchOneOf(search.filter, ISSUE_VIEWS) ??
    'open';
  return {
    view: view === 'open' ? undefined : view,
    ...table.validate(search),
  };
}

export const Route = createFileRoute('/_authenticated/projects/$id/issues/')({
  validateSearch: validateIssuesSearch,
  loaderDeps: ({ search }) => search,
  loader: ({ context: { queryClient }, params, deps }) => {
    const projectId = params.id;
    void queryClient.query(issuesList(projectId, deps)).catch(noop);
    startIfAllowed(
      queryClient,
      (p) => p.can('project-members', 'read', projectId),
      () => void queryClient.query(userQueries.members(projectId)).catch(noop),
    );
    void queryClient.query(projectQueries.environments(projectId)).catch(noop);
    void queryClient
      .query(projectQueries.releaseVersions(projectId))
      .catch(noop);
    void queryClient
      .query(savedViewQueries.list(projectId, 'issues'))
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
  const { id: projectId } = Route.useParams();
  const live = Route.useSearch();
  const search = useDeferredValue(live);
  const { data: project } = useSuspenseQuery(projectQueries.detail(projectId));
  const { data: page } = useSuspenseQuery(issuesList(projectId, search));
  const entry = useEntryView(projectId, 'issues', live, validateIssuesSearch);

  // A stale or shared `?page=` past the end lands on the last page. Only once
  // the rows on screen are the address's.
  const last = pageBeyondEnd(
    table.read(search).pagination.pageIndex + 1,
    page.total_pages,
  );
  if (entry) {
    return (
      <Navigate
        to="/projects/$id/issues"
        params={{ id: projectId }}
        search={entry}
        replace
      />
    );
  }
  if (search === live && last !== false) {
    return (
      <Navigate
        to="/projects/$id/issues"
        params={{ id: projectId }}
        search={{ ...search, page: last }}
        replace
      />
    );
  }

  return (
    <IssuesScreen project={project} page={page} loading={search !== live} />
  );
}

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
  const { id: projectId } = Route.useParams();
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  // The skeleton reads none of the loader's side reads: an observer that
  // unmounts mid-flight cancels the request, and the page would ask again.
  // A disabled observer still counts, so the skeleton does not subscribe.
  // The skeleton is "no project", not `loading`: that is also true for a
  // render while a navigation's deferred search catches up, and toggling
  // `subscribed` then dropped the last observer of every read in flight,
  // which cancelled them and asked again.
  const idle = { enabled: !!project, subscribed: !!project };
  const { me, members } = useAssignable(projectId, {
    ...userQueries.members(projectId),
    ...idle,
  });
  const view = search.view ?? 'open';
  const query = table.read(search);
  // Suggestions only: without them the list still works.
  const environments =
    useQuery({ ...projectQueries.environments(projectId), ...idle }).data ?? [];
  const releases =
    useQuery({ ...projectQueries.releaseVersions(projectId), ...idle }).data ??
    [];

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
          update(table.read(previous)),
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
        actions={
          // Not `loading`: that is also true while a new filter loads, and
          // unmounting the menu then forgets the view being edited.
          !project ? undefined : (
            <SavedViews
              projectId={projectId}
              page="issues"
              search={search}
              onApply={(next) =>
                navigate({ search: validateIssuesSearch(next) })
              }
            />
          )
        }
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
          members={members}
          environments={environments}
          releases={releases}
        />
      </Page>
    </div>
  );
}
