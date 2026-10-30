import type { UpdateIssueState } from '@rustrak/client';
import {
  cn,
  EmptyIcon,
  Page,
  PageHeader,
  Skeleton,
  Tab,
  TabList,
  Tabs,
  Text,
} from '@rustrak/ui';
import { noop, useMutation, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { type ReactNode, useDeferredValue } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { issueMutations } from '@/features/issue/api/mutations';
import { issueQueries } from '@/features/issue/api/queries';
import {
  landingAfter,
  MAILBOX_FILTER,
  MAILBOXES,
} from '@/features/issue/model/feedback-mailbox';
import { FeedbackDetail } from '@/features/issue/ui/components/feedback-detail';
import { FeedbackList } from '@/features/issue/ui/components/feedback-list';
import { useShortcuts } from '@/features/issue/ui/components/issue-actions';
import { projectQueries } from '@/features/project/api/queries';
import {
  searchOneOf,
  searchPage,
  searchString,
} from '@/shared/lib/search-params';
import { PageLoading } from '@/shared/ui/components/page-loading';

/** The page of a mailbox the URL shows, for the loader and the component. */
function feedbackList(
  projectId: number,
  deps: { mailbox: keyof typeof MAILBOX_FILTER; page: number },
) {
  return issueQueries.feedback(projectId, {
    filter: MAILBOX_FILTER[deps.mailbox],
    page: deps.page,
    per_page: 25,
  });
}

/**
 * User feedback: Sentry's feedback inbox. Every feedback is its own issue of
 * category `feedback`, kept out of the issue list and listed here instead.
 *
 * A mail client's layout: the list on the left, the open one on the right,
 * and triage on the keyboard -- j and k to move, r to resolve, e for spam.
 * A feedback that leaves the mailbox hands the pane to the next one, so a
 * backlog is read top to bottom without reaching for the mouse.
 */
export const Route = createFileRoute('/_authenticated/projects/$id/feedback/')({
  validateSearch: (search: Record<string, unknown>) => ({
    mailbox: searchOneOf(search.mailbox, MAILBOXES),
    page: searchPage(search.page),
    selected: searchString(search.selected),
  }),
  loaderDeps: ({ search }) => ({
    mailbox: search.mailbox ?? 'inbox',
    page: search.page ?? 1,
  }),
  loader: ({ context: { queryClient }, params, deps }) => {
    const projectId = Number.parseInt(params.id, 10);
    void queryClient.query(projectQueries.detail(projectId)).catch(noop);
    void queryClient.query(feedbackList(projectId, deps)).catch(noop);
  },
  pendingComponent: FeedbackLoading,
  component: FeedbackPage,
});

/**
 * The list follows the mailbox and the page a beat behind: switching either
 * keeps the current list on screen, dimmed, until the next one is in. Which
 * row is open is read live, so opening one never waits.
 */
function FeedbackPage() {
  const t = useTranslations('projectPages');
  const tIssues = useTranslations('issues');
  const { id } = Route.useParams();
  const search = Route.useSearch();
  const liveMailbox = search.mailbox ?? 'inbox';
  const livePage = search.page ?? 1;
  const mailbox = useDeferredValue(liveMailbox);
  const page = useDeferredValue(livePage);
  const navigate = Route.useNavigate();
  const isLoading = mailbox !== liveMailbox || page !== livePage;
  const projectId = Number.parseInt(id, 10);
  const { data: project } = useSuspenseQuery(projectQueries.detail(projectId));
  const { data: feedback } = useSuspenseQuery(
    feedbackList(projectId, { mailbox, page }),
  );

  const items = feedback.items;
  const ids = items.map((item) => item.id);
  // One is always open: the one the URL names, else the newest.
  const current = items.find((item) => item.id === search.selected) ?? items[0];
  // Narrow, the pane shows only once the reader picked a row on this page;
  // a stale `?selected=` falls back to the list, not to `items[0]`'s pane.
  const opened = current !== undefined && current.id === search.selected;
  const update = useMutation({
    ...issueMutations.update(projectId),
    // Taken when the verb is pressed: the landing belongs to the row acted on.
    onMutate: ({ issueId }) => ({
      search,
      landing: landingAfter(ids, issueId, page),
    }),
    // Before the refetch the mutation names, so the pane has moved on by the
    // time the row leaves the list.
    onSuccess: (_data, _variables, acted) =>
      navigate({
        // A reader who opened another row, page or mailbox while this saved
        // keeps it.
        search: (prev) =>
          prev.selected !== acted.search.selected ||
          prev.mailbox !== acted.search.mailbox ||
          prev.page !== acted.search.page
            ? prev
            : {
                ...prev,
                page: acted.landing.page === 1 ? undefined : acted.landing.page,
                selected: acted.landing.selected,
              },
        replace: true,
      }),
    onError: (error) =>
      toast.error(tIssues('toasts.updateFailed'), {
        description: error.message,
      }),
  });
  const isPending = isLoading || update.isPending;

  const select = (selected: string | undefined) =>
    navigate({ search: (prev) => ({ ...prev, selected }), replace: true });

  const step = (by: number) => {
    const next = current && ids[ids.indexOf(current.id) + by];
    if (next) select(next);
  };

  // Either verb moves the feedback to another mailbox, so the pane moves on.
  const change = (state: UpdateIssueState) => {
    if (!current || isPending) return;
    update.mutate({ issueId: current.id, state });
  };
  const resolve = () =>
    change({
      status: current?.status === 'resolved' ? 'unresolved' : 'resolved',
    });
  const spam = () =>
    change(
      current?.status === 'ignored'
        ? { status: 'unresolved' }
        : { status: 'ignored', substatus: 'archived_until_escalating' },
    );

  useShortcuts({ j: () => step(1), k: () => step(-1), r: resolve, e: spam });

  return (
    <FeedbackScreen
      summary={[
        t('feedback.subtitle', { project: project.name }),
        t('feedback.count', { count: feedback.total_count }),
      ]}
    >
      <title>{t('projectTitle', { project: project.name })}</title>
      {current ? (
        <Page scroll={false} className="lg:flex-row">
          {/* Narrow, the pane replaces the list once the reader picks one. */}
          <div
            className={cn(
              'min-h-0 w-full flex-1 flex-col overflow-hidden rounded-lg border border-border bg-panel lg:flex lg:w-96 lg:flex-none xl:w-[28rem]',
              opened ? 'hidden' : 'flex',
            )}
          >
            <FeedbackList
              feedback={feedback}
              selectedId={current.id}
              page={page}
              isPending={isPending}
              onSelect={select}
              onPageChange={(next) =>
                navigate({
                  search: (prev) => ({
                    ...prev,
                    page: next === 1 ? undefined : next,
                    selected: undefined,
                  }),
                })
              }
            />
          </div>
          <div
            className={cn(
              // A card beside the list; on its own on a phone, the page is the card.
              'min-h-0 flex-1 flex-col overflow-hidden lg:flex lg:rounded-lg lg:border lg:border-border lg:bg-panel',
              opened ? 'flex' : 'hidden',
            )}
          >
            <FeedbackDetail
              key={current.id}
              projectId={projectId}
              feedback={current}
              isPending={isPending}
              onResolve={resolve}
              onSpam={spam}
              onBack={() => select(undefined)}
            />
          </div>
        </Page>
      ) : (
        <div className="flex flex-1 flex-col items-center justify-center gap-2 px-page-gutter py-16 text-center">
          <EmptyIcon size="xl" className="mb-2 text-fg-ghost" />
          <Text variant="body">{t('feedback.empty.title')}</Text>
          <Text variant="meta" tone="subtle" className="max-w-sm">
            {t('feedback.empty.description')}
          </Text>
        </div>
      )}
    </FeedbackScreen>
  );
}

/**
 * The same mailboxes while the project and the list load: the tabs already
 * switch, the rows and the open feedback are outlines.
 */
function FeedbackLoading() {
  return (
    <PageLoading>
      <FeedbackScreen
        summary={[<Skeleton key="summary" className="h-4 w-56" />]}
      >
        <Page scroll={false} className="lg:flex-row">
          <ul className="min-h-0 w-full flex-1 divide-y divide-border-divider overflow-hidden rounded-lg border border-border bg-panel lg:w-96 lg:flex-none xl:w-[28rem]">
            {Array.from({ length: 6 }, (_, row) => (
              // Placeholder rows have no identity beyond their position.
              // react-doctor-disable-next-line react-doctor/no-array-index-as-key
              <li key={row} className="flex gap-3 px-4 py-3">
                <Skeleton shape="circle" className="mt-0.5 size-6" />
                <div className="flex min-w-0 flex-1 flex-col gap-1.5">
                  <Skeleton className="h-4 w-32" />
                  <Skeleton className="h-3 w-full" />
                  <Skeleton className="h-3 w-2/3" />
                </div>
              </li>
            ))}
          </ul>
          <div className="min-h-0 flex-1 flex-col gap-3 overflow-hidden p-5 max-lg:hidden lg:flex lg:rounded-lg lg:border lg:border-border lg:bg-panel">
            <Skeleton className="h-5 w-48" />
            <Skeleton className="h-3 w-32" />
            <Skeleton className="mt-3 h-24 w-full" />
          </div>
        </Page>
      </FeedbackScreen>
    </PageLoading>
  );
}

/** The header and its mailboxes, which read and write the live address. */
function FeedbackScreen({
  summary,
  children,
}: {
  summary: ReactNode[];
  children: ReactNode;
}) {
  const t = useTranslations('projectPages');
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const mailbox = search.mailbox ?? 'inbox';

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title={t('feedback.title')}
        summary={summary}
        tabs={
          <Tabs
            value={mailbox}
            onValueChange={(next) =>
              navigate({
                search: {
                  mailbox:
                    next === 'inbox' ? undefined : (next as typeof mailbox),
                  page: undefined,
                  selected: undefined,
                },
              })
            }
          >
            <TabList>
              {MAILBOXES.map((box) => (
                <Tab key={box} value={box}>
                  {t(`feedback.mailbox.${box}`)}
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
