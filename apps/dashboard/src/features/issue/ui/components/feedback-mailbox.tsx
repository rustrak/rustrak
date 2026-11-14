import type { FeedbackListResponse, IssueStatusUpdate } from '@rustrak/client';
import { cn, Page, Skeleton, Tab, TabList, Tabs } from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { useTranslations } from 'use-intl';
import { issueMutations } from '@/features/issue/api/mutations';
import {
  landingAfter,
  MAILBOXES,
  type Mailbox,
  toggleSpam,
} from '@/features/issue/model/feedback-mailbox';
import { FeedbackDetail } from '@/features/issue/ui/components/feedback-detail';
import { FeedbackList } from '@/features/issue/ui/components/feedback-list';
import { EmptyState } from '@/shared/ui/components/empty-state';
import { usePermissionsLater } from '@/shared/ui/hooks/use-permissions';
import { useShortcuts } from '@/shared/ui/hooks/use-shortcuts';
import { useToastFailure } from '@/shared/ui/hooks/use-toast-failure';

/** The address a mailbox reads: a project's, or every project's. */
export interface MailboxSearch {
  mailbox?: Mailbox;
  page?: number;
  selected?: string;
}

/** Writes the address: what changes, from what it is when it lands. */
export type OnSearch = (
  update: (previous: MailboxSearch) => MailboxSearch,
  replace?: boolean,
) => void;

/**
 * User feedback: Sentry's feedback inbox, for one project or for all of them.
 * Every feedback is its own issue of category `feedback`, kept out of the
 * issue list and listed here instead.
 *
 * A mail client's layout: the list on the left, the open one on the right,
 * and triage on the keyboard -- j and k to move, r to resolve, e for spam.
 * A feedback that leaves the mailbox hands the pane to the next one, so a
 * backlog is read top to bottom without reaching for the mouse.
 *
 * The list follows the mailbox and the page a beat behind (`page` is the
 * deferred one); which row is open is read live, so opening one never waits.
 */
export function FeedbackMailbox({
  feedback,
  search,
  page,
  isLoading,
  onSearch,
  projectNames,
}: {
  feedback: FeedbackListResponse;
  search: MailboxSearch;
  page: number;
  isLoading: boolean;
  onSearch: OnSearch;
  /** Across projects, each row names the project it came from. */
  projectNames?: ReadonlyMap<number, string>;
}) {
  const t = useTranslations('projectPages');
  const tIssues = useTranslations('issues');
  const toastFailure = useToastFailure();
  const permissions = usePermissionsLater();

  const items = feedback.items;
  const ids = items.map((item) => item.id);
  // One is always open: the one the URL names, else the newest.
  const current = items.find((item) => item.id === search.selected) ?? items[0];
  // Narrow, the pane shows only once the reader picked a row on this page;
  // a stale `?selected=` falls back to the list, not to `items[0]`'s pane.
  const opened = current !== undefined && current.id === search.selected;
  const update = useMutation({
    ...issueMutations.bulkUpdate(),
    // Taken when the verb is pressed: the landing belongs to the row acted on.
    onMutate: ({ issues }) => ({
      search,
      landing: landingAfter(ids, issues[0]?.id ?? '', page),
    }),
    // Before the refetch the mutation names, so the pane has moved on by the
    // time the row leaves the list.
    onSuccess: (_data, _variables, acted) =>
      onSearch(
        // A reader who opened another row, page or mailbox while this saved
        // keeps it.
        (prev) =>
          prev.selected !== acted.search.selected ||
          prev.mailbox !== acted.search.mailbox ||
          prev.page !== acted.search.page
            ? prev
            : {
                ...prev,
                page: acted.landing.page === 1 ? undefined : acted.landing.page,
                selected: acted.landing.selected,
              },
        true,
      ),
    onError: (error) => toastFailure(tIssues('toasts.updateFailed'), error),
  });
  const isPending = isLoading || update.isPending;
  const canTriage =
    current !== undefined &&
    permissions.can('issues', 'update', current.project_id);

  const select = (selected: string | undefined) =>
    onSearch((prev) => ({ ...prev, selected }), true);

  const step = (by: number) => {
    const next = current && ids[ids.indexOf(current.id) + by];
    if (next) select(next);
  };

  // Either verb moves the feedback to another mailbox, so the pane moves on.
  const change = (status: IssueStatusUpdate) => {
    // The keys do nothing for a reader who may not triage; the buttons are
    // not shown to them.
    if (!current || isPending || !canTriage) return;
    update.mutate({ issues: [current], status });
  };
  const resolve = () =>
    change(current?.status === 'resolved' ? 'unresolved' : 'resolved');
  const spam = () => {
    const next = current && toggleSpam(current.status).status;
    if (next) change(next);
  };

  useShortcuts({ j: () => step(1), k: () => step(-1), r: resolve, e: spam });

  return current ? (
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
          projectNames={projectNames}
          onSelect={select}
          onPageChange={(next) =>
            onSearch((prev) => ({
              ...prev,
              page: next === 1 ? undefined : next,
              selected: undefined,
            }))
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
          projectId={current.project_id}
          feedback={current}
          isPending={isPending}
          onResolve={resolve}
          onSpam={spam}
          onBack={() => select(undefined)}
        />
      </div>
    </Page>
  ) : (
    <EmptyState
      title={t('feedback.empty.title')}
      description={t('feedback.empty.description')}
    />
  );
}

/** The rows and the open feedback as outlines, while the list loads. */
export function FeedbackMailboxSkeleton() {
  return (
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
  );
}

/** Which mailbox, as the page's tabs. */
export function MailboxSwitch({
  search,
  onSearch,
}: {
  search: MailboxSearch;
  onSearch: OnSearch;
}) {
  const t = useTranslations('projectPages');
  const mailbox = search.mailbox ?? 'inbox';
  return (
    <Tabs
      value={mailbox}
      onValueChange={(next) =>
        onSearch((prev) => ({
          ...prev,
          mailbox: next === 'inbox' ? undefined : (next as Mailbox),
          page: undefined,
          selected: undefined,
        }))
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
  );
}
