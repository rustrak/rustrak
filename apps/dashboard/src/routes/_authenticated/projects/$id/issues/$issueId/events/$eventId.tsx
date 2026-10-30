import {
  Button,
  CollapsePanelIcon,
  cn,
  ExpandPanelIcon,
  Tooltip,
} from '@rustrak/ui';
import { noop, useQuery, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { type ReactNode, useDeferredValue, useState } from 'react';
import { useTranslations } from 'use-intl';
import { eventQueries } from '@/features/event/api/queries';
import {
  type EventJumpTarget,
  eventJumpTargets,
} from '@/features/event/lib/event-jumps';
import {
  readEventPayload,
  splitIssueTitle,
} from '@/features/event/lib/event-payload';
import { EventLoading } from '@/features/event/ui/components/event-loading';
import { githubQueries } from '@/features/github/api/queries';
import { frameSourceUrl } from '@/features/github/lib/source-url';
import { IssueGitHubButton } from '@/features/github/ui/components/issue-github-button';
import { issueQueries } from '@/features/issue/api/queries';
import { assignable } from '@/features/issue/model/assignable';
import { projectQueries } from '@/features/project/api/queries';
import { userQueries } from '@/features/user/api/queries';
import { usePageTrail } from '@/shared/ui/hooks/use-page-trail';
import { EventNavigator } from './$eventId/-components/event-navigator';
import { EventRail } from './$eventId/-components/event-rail';
import { EventSections } from './$eventId/-components/event-sections';
import { EventTrends } from './$eventId/-components/event-trends';
import { IssueHeader } from './$eventId/-components/issue-header';

/** The page's reads, built once for the loader and the component. */
function eventReads(projectId: number, issueId: string, eventId: string) {
  return {
    issue: issueQueries.detail(projectId, issueId),
    event: eventQueries.detail(projectId, issueId, eventId),
    navigation: eventQueries.navigation(projectId, issueId, eventId),
    repository: githubQueries.repository(projectId),
    userReports: issueQueries.userReports(projectId, issueId),
    attachments: eventQueries.attachments(projectId, issueId, eventId),
    stats30d: issueQueries.stats(projectId, issueId, '30d'),
    aggregates: issueQueries.aggregates(projectId, issueId),
    activity: issueQueries.activity(projectId, issueId),
    code: githubQueries.issue(projectId, issueId),
    members: userQueries.members(projectId),
  };
}

export const Route = createFileRoute(
  '/_authenticated/projects/$id/issues/$issueId/events/$eventId',
)({
  // Every read is started here and none is waited on. The event and what
  // frames it are what the page suspends on; the trends, the rail and the
  // GitHub button render empty if they fail: an issue with no tags and a
  // failed aggregates read genuinely render the same panel, and neither is
  // worth taking the event view down for.
  loader: ({ context: { queryClient }, params }) => {
    const projectId = Number.parseInt(params.id, 10);
    const reads = eventReads(projectId, params.issueId, params.eventId);
    void queryClient.query(projectQueries.detail(projectId)).catch(noop);
    void queryClient.query(reads.issue).catch(noop);
    void queryClient.query(reads.event).catch(noop);
    void queryClient.query(reads.navigation).catch(noop);
    void queryClient.query(reads.repository).catch(noop);
    void queryClient.infiniteQuery(reads.userReports).catch(noop);
    void queryClient.query(reads.attachments).catch(noop);
    void queryClient.query(reads.stats30d).catch(noop);
    void queryClient.query(reads.aggregates).catch(noop);
    void queryClient.query(reads.activity).catch(noop);
    void queryClient.query(reads.code).catch(noop);
    void queryClient.query(reads.members).catch(noop);
  },
  pendingComponent: EventLoading,
  component: EventView,
});

/**
 * The event, a beat behind the address: stepping to the next event, or to
 * another issue, keeps the one on screen, dimmed, until the next is in,
 * rather than swapping the whole view for its skeleton.
 */
function EventView() {
  const live = Route.useParams();
  const params = useDeferredValue(live);
  const { id, issueId, eventId } = params;
  const context = Route.useRouteContext();
  const projectId = Number.parseInt(id, 10);
  const reads = eventReads(projectId, issueId, eventId);
  const { data: project } = useSuspenseQuery(projectQueries.detail(projectId));
  const { data: issue } = useSuspenseQuery(reads.issue);
  const { data: event } = useSuspenseQuery(reads.event);
  const { data: navigation } = useSuspenseQuery(reads.navigation);
  const { data: repository } = useSuspenseQuery(reads.repository);
  const { data: attachments } = useSuspenseQuery(reads.attachments);
  const stats30d = useQuery(reads.stats30d).data ?? null;
  const aggregates = useQuery(reads.aggregates).data ?? null;
  const activity = useQuery(reads.activity).data ?? [];
  const code = useQuery(reads.code).data ?? null;
  const user = context.state === 'authenticated' ? context.user : null;
  const members = assignable(useQuery(reads.members).data ?? [], user);
  const me = user?.id ?? 0;
  const t = useTranslations('projectPages');
  usePageTrail([{ label: issue.short_id, mono: true }]);
  const eventData = event.data as Record<string, unknown>;
  const payload = readEventPayload(eventData);
  const { has } = payload;
  const { type: titleType, message } = splitIssueTitle(
    issue.title,
    issue.value,
  );
  // One entry per section the event actually has. Labelled here rather than
  // in `eventJumpTargets` so every message key stays a literal the
  // message-keys architecture rule can check.
  const SECTION_LABELS: Record<EventJumpTarget, string> = {
    highlights: t('event.sectionHighlights'),
    stacktrace: t('event.sectionStackTrace'),
    breadcrumbs: t('event.sectionBreadcrumbs'),
    tags: t('event.sectionTags'),
    context: t('event.sectionContext'),
  };
  const jumps = eventJumpTargets(has).map((id) => ({
    id,
    label: SECTION_LABELS[id],
  }));

  const rail = (
    <EventRail
      projectId={projectId}
      issueId={issueId}
      issue={issue}
      aggregates={aggregates}
      activity={activity}
      code={code}
      connected={repository !== null}
      members={members}
      me={me}
    />
  );
  const sourceUrl = repository
    ? (frame: { filename?: string; lineno?: number | null }) =>
        frameSourceUrl(repository, frame.filename, frame.lineno)
    : undefined;
  // Links outlive a disconnected repository and can still be unlinked.
  // Unloaded links are not "no links": offering Create then could open a
  // second GitHub issue for one already linked.
  const githubLinks =
    code && (repository || code.links.length > 0) ? code.links : null;

  return (
    // On a phone the whole page scrolls as one; from lg the reading column
    // and the rail scroll on their own, under a header that stays.
    <div
      aria-busy={params !== live}
      className={cn(
        'flex h-full flex-col max-lg:overflow-y-auto',
        params !== live && 'opacity-60',
      )}
    >
      <title>{t('projectTitle', { project: project.name })}</title>
      <IssueHeader
        issue={issue}
        projectId={projectId}
        me={me}
        titleType={titleType}
        message={message}
        github={
          githubLinks ? (
            <IssueGitHubButton
              projectId={projectId}
              issueId={issue.id}
              links={githubLinks}
              canLink={repository != null}
            />
          ) : undefined
        }
      />

      <div className="flex min-h-0 flex-1 max-lg:flex-col">
        <main className="min-w-0 flex-1 lg:overflow-y-auto">
          <div className="flex flex-col gap-5 px-page-gutter pb-8 pt-5">
            <EventTrends stats={stats30d} />

            <EventNavigator
              projectId={projectId}
              issueId={issueId}
              event={event}
              navigation={navigation}
              jumps={jumps}
            />

            <EventSections
              projectId={projectId}
              issueId={issueId}
              attachments={attachments}
              event={event}
              payload={payload}
              eventData={eventData}
              sourceUrl={sourceUrl}
            />
          </div>
        </main>

        <IssueRail title={t('event.details')}>{rail}</IssueRail>
      </div>
    </div>
  );
}

/** Whether the reader last left the rail folded, in this browser. */
function railFolded(): boolean {
  try {
    return window.localStorage.getItem('rustrak:issue-rail') === 'folded';
  } catch {
    return false;
  }
}

/**
 * The rail, foldable from lg up. The arrows point where the rail goes: out
 * to the right when folding, back in from it when opening.
 *
 * Foldable so a long stack trace can have the width.
 * Folded, it leaves a strip with the way back in the same place; the choice
 * is remembered, since a reader who folds it once wants it folded on the
 * next issue too. Below lg it is not beside the page but under it, so there
 * is nothing to fold.
 */
function IssueRail({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  const tCommon = useTranslations('common');
  const [folded, setFolded] = useState(railFolded);

  // Remembered in the handler, not in the state updater: React may run an
  // updater twice, and writing storage is not something to do twice.
  const toggle = () => {
    const next = !folded;
    setFolded(next);
    try {
      window.localStorage.setItem(
        'rustrak:issue-rail',
        next ? 'folded' : 'open',
      );
    } catch {
      // Storage unavailable: the fold lasts as long as the page.
    }
  };

  return (
    <>
      {/* Folding is a desktop layout: below `lg` the rail is a section under
          the reading column with no way to unfold it, so there it is always
          shown, whatever was folded on a wider screen. */}
      {folded ? (
        <div className="hidden w-12 shrink-0 justify-center border-border-divider border-l bg-panel pt-3 lg:flex">
          <Tooltip content={tCommon('showSidebar')} side="left">
            <Button
              variant="ghost"
              size="sm"
              icon={CollapsePanelIcon}
              aria-label={tCommon('openSidebar')}
              onClick={toggle}
            />
          </Tooltip>
        </div>
      ) : null}
      <RailBody title={title} folded={folded} onFold={toggle}>
        {children}
      </RailBody>
    </>
  );
}

function RailBody({
  title,
  folded,
  onFold,
  children,
}: {
  title: string;
  folded: boolean;
  onFold: () => void;
  children: ReactNode;
}) {
  const tCommon = useTranslations('common');
  return (
    <aside
      aria-label={title}
      className={cn(
        'border-border-divider bg-panel max-lg:border-t lg:w-80 lg:shrink-0 lg:overflow-y-auto lg:border-l xl:w-88',
        folded && 'lg:hidden',
      )}
    >
      <div className="sticky top-0 z-10 hidden h-11 items-center justify-between border-border-divider border-b bg-panel ps-5 pe-3 lg:flex">
        <span className="text-card-title text-fg">{title}</span>
        <Tooltip content={tCommon('hideSidebar')} side="left">
          <Button
            variant="ghost"
            size="sm"
            icon={ExpandPanelIcon}
            aria-label={tCommon('closeSidebar')}
            onClick={onFold}
          />
        </Tooltip>
      </div>
      {children}
    </aside>
  );
}
