import type { IssuesListResponse } from '@rustrak/client';
import {
  Count,
  type DataTableQuery,
  Page,
  PageHeader,
  Separator,
  Skeleton,
  Tab,
  TabList,
  Tabs,
} from '@rustrak/ui';
import {
  noop,
  useQueries,
  useQuery,
  useSuspenseQuery,
} from '@tanstack/react-query';
import { createFileRoute, Navigate } from '@tanstack/react-router';
import { type ReactNode, useDeferredValue } from 'react';
import { useFormatter, useTranslations } from 'use-intl';
import { issueQueries } from '@/features/issue/api/queries';
import { assignable, everyone } from '@/features/issue/model/assignable';
import {
  MAILBOX_FILTER,
  MAILBOXES,
  type Mailbox,
} from '@/features/issue/model/feedback-mailbox';
import {
  ISSUE_FILTER_VARIANTS,
  ISSUE_VIEWS,
  type IssueView,
  issueListParams,
  withServerQuery,
} from '@/features/issue/model/issue-table';
import {
  FeedbackMailbox,
  FeedbackMailboxSkeleton,
  type OnSearch,
} from '@/features/issue/ui/components/feedback-mailbox';
import { IssuesTable } from '@/features/issue/ui/components/issues-table/issues-table';
import { projectQueries } from '@/features/project/api/queries';
import { savedViewQueries } from '@/features/saved-view/api/queries';
import { SavedViews } from '@/features/saved-view/ui/components/saved-views';
import { useEntryView } from '@/features/saved-view/ui/hooks/use-entry-view';
import { userQueries } from '@/features/user/api/queries';
import {
  searchIds,
  searchOneOf,
  searchString,
} from '@/shared/lib/search-params';
import {
  pageBeyondEnd,
  type TableSearch,
  tableSearch,
} from '@/shared/lib/table-search';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { ProjectsPicker } from '@/shared/ui/components/projects-picker';
import { usePermissionsLater } from '@/shared/ui/hooks/use-permissions';
import { useSessionUser } from '@/shared/ui/hooks/use-session-user';

const PAGE_SIZE = 50;
const FEEDBACK_PAGE_SIZE = 25;
const TABS = ['errors', 'feedback'] as const;
type IssuesTab = (typeof TABS)[number];

/** Who an issue can be assigned to across projects: the reader alone. */
const NO_MEMBERS: never[] = [];
const NO_SUGGESTIONS: string[] = [];

/**
 * One address for both tabs. `project` (`1,2`) is theirs in common, so a
 * switch of tab keeps it; the rest belongs to whichever tab is open, and a
 * switch drops it. `page` is the open tab's page.
 */
interface IssuesSearch extends TableSearch {
  tab?: 'feedback';
  project?: string;
  view?: IssueView;
  mailbox?: Mailbox;
  selected?: string;
}

/** The address less the open feedback: a view is what is asked for. */
const viewable = (search: IssuesSearch) => ({ ...search, selected: undefined });

const table = tableSearch({
  variants: ISSUE_FILTER_VARIANTS,
  pageSize: PAGE_SIZE,
  normalise: withServerQuery,
});

/** The `project` parameter both lists send, when the reader narrowed them. */
const inProjects = (search: IssuesSearch) =>
  search.project ? { project: search.project } : {};

function errorsList(search: IssuesSearch) {
  return issueQueries.listAll({
    ...issueListParams(table.read(search), search.view ?? 'open'),
    ...inProjects(search),
  });
}

function feedbackList(search: IssuesSearch) {
  return issueQueries.feedbackAll({
    filter: MAILBOX_FILTER[search.mailbox ?? 'inbox'],
    page: search.page ?? 1,
    per_page: FEEDBACK_PAGE_SIZE,
    ...inProjects(search),
  });
}

/** What is waiting in each tab, for its count: open errors, unread feedback. */
const waiting = {
  errors: (search: IssuesSearch) =>
    issueQueries.listAll({
      filter: 'open',
      per_page: 1,
      ...inProjects(search),
    }),
  feedback: (search: IssuesSearch) =>
    issueQueries.feedbackAll({
      filter: 'open',
      per_page: 1,
      ...inProjects(search),
    }),
};

/** The address as typed search; a saved view's text goes through it too. */
function validateIssuesSearch(search: Record<string, unknown>): IssuesSearch {
  const tab = searchOneOf(search.tab, TABS);
  const project = searchIds(search.project)?.join(',');
  if (tab === 'feedback') {
    const mailbox = searchOneOf(search.mailbox, MAILBOXES);
    const { page } = table.validate(search);
    return {
      tab,
      project,
      mailbox: mailbox === 'inbox' ? undefined : mailbox,
      page,
      selected: searchString(search.selected),
    };
  }
  const view = searchOneOf(search.view, ISSUE_VIEWS);
  return {
    project,
    view: view === 'open' ? undefined : view,
    ...table.validate(search),
  };
}

/**
 * Every project's issues and user feedback, on one page: Sentry's issue
 * stream, with its feedback inbox as the second tab. The projects picked in
 * the header narrow both, and each row names the project it belongs to.
 */
export const Route = createFileRoute('/_authenticated/issues/')({
  validateSearch: validateIssuesSearch,
  loaderDeps: ({ search }) => search,
  loader: ({ context: { queryClient }, deps }) => {
    if (deps.tab === 'feedback') {
      void queryClient.query(feedbackList(deps)).catch(noop);
    } else {
      void queryClient.query(errorsList(deps)).catch(noop);
    }
    void queryClient.query(waiting.errors(deps)).catch(noop);
    void queryClient.query(waiting.feedback(deps)).catch(noop);
    void queryClient.query(projectQueries.picker()).catch(noop);
    void queryClient
      .query(savedViewQueries.list(undefined, 'issues'))
      .catch(noop);
  },
  pendingComponent: IssuesLoading,
  component: IssuesPage,
});

function useOnSearch(): OnSearch {
  const navigate = Route.useNavigate();
  return (update, replace) =>
    navigate({ search: (prev) => ({ ...prev, ...update(prev) }), replace });
}

/**
 * The open tab follows the address a beat behind: a new filter, page or tab
 * keeps what is on screen, dimmed, until the next list is in.
 */
function IssuesPage() {
  const live = Route.useSearch();
  const search = useDeferredValue(live);
  const entry = useEntryView(undefined, 'issues', viewable(live));
  if (entry) {
    return (
      <Navigate to="/issues" search={validateIssuesSearch(entry)} replace />
    );
  }
  return search.tab === 'feedback' ? (
    <FeedbackTab search={search} loading={search !== live} />
  ) : (
    <ErrorsTab search={search} loading={search !== live} />
  );
}

function IssuesLoading() {
  const tab = Route.useSearch().tab ?? 'errors';
  return (
    <PageLoading>
      <IssuesFrame summary={null} loading>
        {tab === 'feedback' ? (
          <FeedbackMailboxSkeleton />
        ) : (
          <ErrorsScreen loading />
        )}
      </IssuesFrame>
    </PageLoading>
  );
}

function ErrorsTab({
  search,
  loading,
}: {
  search: IssuesSearch;
  loading: boolean;
}) {
  const tIssues = useTranslations('issues');
  const format = useFormatter();
  const { data: page } = useSuspenseQuery(errorsList(search));

  // A stale or shared `?page=` past the end lands on the last page. Only once
  // the rows on screen are the address's.
  const last = pageBeyondEnd(
    table.read(search).pagination.pageIndex + 1,
    page.total_pages,
  );
  if (!loading && last !== false) {
    return <Navigate to="/issues" search={{ ...search, page: last }} replace />;
  }

  return (
    <IssuesFrame
      summary={tIssues('table.count', {
        count: page.total_count,
        display: format.number(page.total_count),
      })}
    >
      <ErrorsScreen page={page} loading={loading} />
    </IssuesFrame>
  );
}

function ErrorsScreen({
  page,
  loading,
}: {
  page?: IssuesListResponse;
  loading: boolean;
}) {
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const me = useSessionUser().id;
  const permissions = usePermissionsLater();
  const projects =
    useQuery({ ...projectQueries.picker(), enabled: !loading }).data?.items ??
    [];
  // Names for the Assignee column: the members of the projects on this page,
  // the same reads each project's own list makes. Only asked of who may read
  // them, since anyone else gets a 403.
  const listed = [...new Set(page?.items.map((i) => i.project_id))].filter(
    (id) => permissions.can('project-members', 'read', id),
  );
  const assignees = everyone(
    useQueries({
      queries: listed.map((id) => ({
        ...userQueries.members(id),
        enabled: !loading,
      })),
    }).map((read) => assignable(read.data ?? [], null)),
  );

  // Each change applies to the address as it is when it lands, not as it was
  // when this render read it.
  const setQuery = (update: (previous: DataTableQuery) => DataTableQuery) =>
    navigate({
      search: (previous) => ({
        project: previous.project,
        view: previous.view,
        ...table.write(update(table.read(previous))),
      }),
    });

  return (
    <Page scroll={false}>
      <IssuesTable
        projects={projects}
        page={page}
        query={table.read(search)}
        view={search.view ?? 'open'}
        onQueryChange={setQuery}
        loading={loading}
        me={me}
        members={NO_MEMBERS}
        assignees={assignees}
        environments={NO_SUGGESTIONS}
        releases={NO_SUGGESTIONS}
      />
    </Page>
  );
}

function FeedbackTab({
  search,
  loading,
}: {
  search: IssuesSearch;
  loading: boolean;
}) {
  const tPages = useTranslations('projectPages');
  const live = Route.useSearch();
  const onSearch = useOnSearch();
  const { data: feedback } = useSuspenseQuery(feedbackList(search));
  const projects = useQuery(projectQueries.picker()).data?.items ?? [];

  return (
    <IssuesFrame
      summary={tPages('feedback.count', { count: feedback.total_count })}
    >
      <FeedbackMailbox
        feedback={feedback}
        search={live}
        page={search.page ?? 1}
        isLoading={loading}
        onSearch={onSearch}
        projectNames={new Map(projects.map((p) => [p.id, p.name]))}
      />
    </IssuesFrame>
  );
}

/**
 * The header both tabs share: the tabs with what waits in each, the open
 * tab's status, and the projects that narrow both.
 */
function IssuesFrame({
  summary,
  loading = false,
  children,
}: {
  summary: ReactNode;
  /** The skeleton: no side reads, so nothing it starts is cancelled. */
  loading?: boolean;
  children: ReactNode;
}) {
  const tWorkspace = useTranslations('workspaceIssues');
  const format = useFormatter();
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const tab: IssuesTab = search.tab ?? 'errors';
  const projects = useQuery(projectQueries.picker()).data?.items ?? [];
  const counts = {
    errors: useQuery(waiting.errors(search)).data?.total_count,
    feedback: useQuery(waiting.feedback(search)).data?.total_count,
  };
  const picked = searchIds(search.project) ?? null;
  const tProjects = useTranslations('projects');
  // What the page covers: every project, or the ones picked, by name while
  // they fit.
  const names = picked?.flatMap(
    (id) => projects.find((p) => p.id === id)?.name ?? [],
  );
  const scope = !picked
    ? tWorkspace('subtitle')
    : names && names.length === picked.length && names.length <= 2
      ? names.join(', ')
      : tProjects('switcher.count', { count: picked.length });

  return (
    <div className="flex h-full flex-col">
      <title>{tWorkspace('title')}</title>
      <PageHeader
        title={tWorkspace('title')}
        summary={[
          scope,
          summary ?? <Skeleton key="summary" className="h-4 w-24" />,
        ]}
        actions={
          <div className="flex items-center gap-2">
            <ProjectsPicker
              compact
              projects={projects}
              value={picked}
              readOnly={false}
              allLabel={tWorkspace('allProjects')}
              allHint={tWorkspace('allProjectsHint')}
              onChange={(next) =>
                navigate({
                  search: (prev) => ({
                    ...prev,
                    project: next?.length ? next.join(',') : undefined,
                    page: undefined,
                    selected: undefined,
                  }),
                })
              }
            />
            {loading ? null : (
              <>
                <Separator orientation="vertical" className="h-5" />
                <SavedViews
                  page="issues"
                  search={viewable(search)}
                  onApply={(next) =>
                    navigate({ search: validateIssuesSearch(next) })
                  }
                />
              </>
            )}
          </div>
        }
        tabs={
          <Tabs
            value={tab}
            onValueChange={(next) =>
              navigate({
                search: (prev) => ({
                  project: prev.project,
                  tab: next === 'feedback' ? 'feedback' : undefined,
                }),
              })
            }
          >
            <TabList>
              {TABS.map((value) => (
                <Tab key={value} value={value}>
                  {tWorkspace(value)}
                  <Count>
                    {counts[value] === undefined
                      ? null
                      : format.number(counts[value])}
                  </Count>
                </Tab>
              ))}
            </TabList>
          </Tabs>
        }
      />
      {children}
    </div>
  );
}
