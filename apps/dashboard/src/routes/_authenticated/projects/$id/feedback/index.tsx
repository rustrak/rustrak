import { PageHeader, Skeleton } from '@rustrak/ui';
import { noop, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { type ReactNode, useDeferredValue } from 'react';
import { useTranslations } from 'use-intl';
import { issueQueries } from '@/features/issue/api/queries';
import {
  MAILBOX_FILTER,
  MAILBOXES,
} from '@/features/issue/model/feedback-mailbox';
import {
  FeedbackMailbox,
  FeedbackMailboxSkeleton,
  MailboxSwitch,
  type OnSearch,
} from '@/features/issue/ui/components/feedback-mailbox';
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

/** A project's feedback inbox; every project's is a tab of `/issues`. */
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
    void queryClient.query(feedbackList(params.id, deps)).catch(noop);
  },
  pendingComponent: FeedbackLoading,
  component: FeedbackPage,
});

function useOnSearch(): OnSearch {
  const navigate = Route.useNavigate();
  return (update, replace) =>
    navigate({ search: (prev) => ({ ...prev, ...update(prev) }), replace });
}

function FeedbackPage() {
  const t = useTranslations('projectPages');
  const { id: projectId } = Route.useParams();
  const search = Route.useSearch();
  const onSearch = useOnSearch();
  const liveMailbox = search.mailbox ?? 'inbox';
  const livePage = search.page ?? 1;
  const mailbox = useDeferredValue(liveMailbox);
  const page = useDeferredValue(livePage);
  const { data: project } = useSuspenseQuery(projectQueries.detail(projectId));
  const { data: feedback } = useSuspenseQuery(
    feedbackList(projectId, { mailbox, page }),
  );

  return (
    <FeedbackFrame
      summary={[
        t('feedback.subtitle', { project: project.name }),
        t('feedback.count', { count: feedback.total_count }),
      ]}
    >
      <title>{t('projectTitle', { project: project.name })}</title>
      <FeedbackMailbox
        feedback={feedback}
        search={search}
        page={page}
        isLoading={mailbox !== liveMailbox || page !== livePage}
        onSearch={onSearch}
      />
    </FeedbackFrame>
  );
}

function FeedbackLoading() {
  return (
    <PageLoading>
      <FeedbackFrame
        summary={[<Skeleton key="summary" className="h-4 w-56" />]}
      >
        <FeedbackMailboxSkeleton />
      </FeedbackFrame>
    </PageLoading>
  );
}

/** The header and its mailboxes, which read and write the live address. */
function FeedbackFrame({
  summary,
  children,
}: {
  summary: ReactNode[];
  children: ReactNode;
}) {
  const t = useTranslations('projectPages');
  const search = Route.useSearch();
  const onSearch = useOnSearch();
  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title={t('feedback.title')}
        summary={summary}
        tabs={<MailboxSwitch search={search} onSearch={onSearch} />}
      />
      {children}
    </div>
  );
}
