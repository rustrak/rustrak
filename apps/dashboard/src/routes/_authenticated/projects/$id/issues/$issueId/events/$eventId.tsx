import type {
  ActivityEntry,
  AttachmentInfo,
  EventDetailResponse,
  IssueAggregates,
  IssueGitHub,
  IssueResponse,
  IssueStatsResponse,
  IssuesListUserReportsResponse,
  ProjectMemberResponse,
  ProjectRepository,
  Result,
  RustrakError,
  UserResponse,
} from '@rustrak/client';
import { Ok } from '@rustrak/client';
import {
  Button,
  CollapsePanelIcon,
  cn,
  ExpandPanelIcon,
  Tooltip,
} from '@rustrak/ui';
import { createFileRoute } from '@tanstack/react-router';
import { type ReactNode, useState } from 'react';
import { useTranslations } from 'use-intl';
import {
  type EventNavigation,
  getEventDetail,
  getEventNavigation,
  listEventAttachments,
} from '@/features/event/api/queries';
import {
  type EventJumpTarget,
  eventJumpTargets,
} from '@/features/event/lib/event-jumps';
import {
  readEventPayload,
  splitIssueTitle,
} from '@/features/event/lib/event-payload';
import {
  getIssueGitHub,
  getProjectRepository,
} from '@/features/github/api/queries';
import { frameSourceUrl } from '@/features/github/lib/source-url';
import { IssueGitHubButton } from '@/features/github/ui/components/issue-github-button';
import {
  getIssue,
  getIssueActivity,
  getIssueAggregates,
  getIssueStats,
  listUserReports,
} from '@/features/issue/api/queries';
import { assignable } from '@/features/issue/model/assignable';
import { getProject } from '@/features/project/api/queries';
import { listProjectMembers } from '@/features/user/api/queries';
import { translator } from '@/shared/i18n/intl';
import { loadAll, orElse } from '@/shared/lib/results';
import { Deferred } from '@/shared/ui/components/deferred';
import { LoadFailure } from '@/shared/ui/components/load-failure';
import { usePageTrail } from '@/shared/ui/hooks/use-page-trail';
import { EventLoading } from './$eventId/-components/event-loading';
import { EventNavigator } from './$eventId/-components/event-navigator';
import { EventRail } from './$eventId/-components/event-rail';
import { EventSections } from './$eventId/-components/event-sections';
import { EventTrends } from './$eventId/-components/event-trends';
import { IssueHeader } from './$eventId/-components/issue-header';

export const Route = createFileRoute(
  '/_authenticated/projects/$id/issues/$issueId/events/$eventId',
)({
  // The event and what places it are awaited; the trends, the rail and the
  // GitHub links arrive on their own. Each of those already degraded when it
  // failed, so `orElse` keeps that: a panel without its read renders empty,
  // never an error.
  loader: async ({ params }) => {
    const projectId = Number.parseInt(params.id, 10);
    const { issueId, eventId } = params;

    const deferred = {
      stats: orElse(getIssueStats(projectId, issueId, '30d'), null),
      aggregates: orElse(getIssueAggregates(projectId, issueId), null),
      activity: orElse(getIssueActivity(projectId, issueId), []),
      code: orElse(getIssueGitHub(projectId, issueId), null),
      members: orElse(listProjectMembers(projectId), []),
    };
    const [
      project,
      issue,
      event,
      navigation,
      repository,
      userReports,
      attachments,
    ] = await Promise.all([
      getProject(projectId),
      getIssue(projectId, issueId),
      getEventDetail(projectId, issueId, eventId),
      getEventNavigation(projectId, issueId, eventId),
      getProjectRepository(projectId),
      listUserReports(projectId, issueId),
      listEventAttachments(projectId, issueId, eventId),
    ]);

    return {
      project,
      issue,
      event,
      navigation,
      repository,
      userReports,
      attachments,
      deferred,
    };
  },
  head: ({ loaderData }) => {
    const t = translator('projectPages');

    if (!loaderData?.project.success || !loaderData.event.success) {
      return { meta: [{ title: t('event.meta.eventNotFound') }] };
    }

    return {
      meta: [
        {
          title: t('projectTitle', { project: loaderData.project.data.name }),
        },
        { name: 'description', content: t('event.meta.description') },
      ],
    };
  },
  // The event view still awaits seven requests, so it is the one screen with
  // a visible wait. This is the same skeleton Next rendered from
  // `loading.tsx`, mounted the same way: after `defaultPendingMs`, so a fast
  // network never flashes it.
  pendingComponent: EventLoading,
  component: EventPage,
});

function EventPage() {
  const t = useTranslations('projectPages');
  const { id, issueId } = Route.useParams();
  const context = Route.useRouteContext();
  const {
    project: projectResult,
    issue: issueResult,
    event: eventResult,
    navigation: navigationResult,
    repository: repositoryResult,
    userReports: userReportsResult,
    attachments: attachmentsResult,
    deferred,
  } = Route.useLoaderData();
  const projectId = Number.parseInt(id, 10);

  // The four the page cannot render without.
  if (!projectResult.success) {
    return (
      <LoadFailure error={projectResult.error} title={t('loadProjectFailed')} />
    );
  }
  if (!issueResult.success) {
    return (
      <LoadFailure error={issueResult.error} title={t('loadIssueFailed')} />
    );
  }
  if (!eventResult.success) {
    return (
      <LoadFailure error={eventResult.error} title={t('event.loadFailed')} />
    );
  }
  if (!navigationResult.success) {
    return (
      <LoadFailure
        error={navigationResult.error}
        title={t('event.loadNavigationFailed')}
      />
    );
  }

  // The rest decorate the page. Each already degraded on failure before
  // the Result conversion; the degradation is now written out rather than
  // hidden behind a `.catch()`, and it stays deliberate: an issue with no tags
  // and an aggregates endpoint that failed genuinely render the same panel, and
  // neither is worth taking the event view down for.
  return (
    <EventView
      projectId={projectId}
      issueId={issueId}
      issue={issueResult.data}
      event={eventResult.data}
      navigation={navigationResult.data}
      repository={repositoryResult.success ? repositoryResult.data : undefined}
      userReports={userReportsResult.success ? userReportsResult.data : null}
      attachments={attachmentsResult.success ? attachmentsResult.data : []}
      deferred={deferred}
      user={context.state === 'authenticated' ? context.user : null}
    />
  );
}

function EventView({
  projectId,
  issueId,
  issue,
  event,
  navigation,
  repository,
  userReports,
  attachments,
  deferred,
  user,
}: {
  projectId: number;
  issueId: string;
  issue: IssueResponse;
  event: EventDetailResponse;
  navigation: EventNavigation;
  /** The project's GitHub repository: `null` without one, `undefined` when
   * it could not be loaded. */
  repository: ProjectRepository | null | undefined;
  /** The newest page of feedback users sent about this issue. */
  userReports: IssuesListUserReportsResponse | null;
  /** Files sent with the event. */
  attachments: AttachmentInfo[];
  deferred: Deferreds;
  user: UserResponse | null;
}) {
  const t = useTranslations('projectPages');
  const me = user?.id ?? 0;
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
    <Deferred
      promise={railData(deferred)}
      title={t('event.details')}
      fallback={
        <div className="m-5 h-80 animate-pulse rounded-lg bg-surface-hover motion-reduce:animate-none" />
      }
    >
      {({ aggregates, activity, code, members }) => (
        <EventRail
          projectId={projectId}
          issueId={issueId}
          issue={issue}
          aggregates={aggregates}
          activity={activity}
          code={code}
          connected={repository === undefined ? null : repository !== null}
          members={assignable(members, user)}
          me={me}
        />
      )}
    </Deferred>
  );
  const sourceUrl = repository
    ? (frame: { filename?: string; lineno?: number | null }) =>
        frameSourceUrl(repository, frame.filename, frame.lineno)
    : undefined;

  return (
    // On a phone the whole page scrolls as one; from lg the reading column
    // and the rail scroll on their own, under a header that stays.
    <div className="flex h-full flex-col max-lg:overflow-y-auto">
      <IssueHeader
        issue={issue}
        projectId={projectId}
        me={me}
        titleType={titleType}
        message={message}
        github={
          <Deferred
            promise={deferred.code}
            title={t('event.details')}
            fallback={null}
          >
            {(code) =>
              // Links outlive a disconnected repository and can still be
              // unlinked. Unloaded links are not "no links": offering Create
              // then could open a second GitHub issue for one already linked.
              code && (repository || code.links.length > 0) ? (
                <IssueGitHubButton
                  projectId={projectId}
                  issueId={issue.id}
                  links={code.links}
                  canLink={repository != null}
                />
              ) : null
            }
          </Deferred>
        }
      />

      <div className="flex min-h-0 flex-1 max-lg:flex-col">
        <main className="min-w-0 flex-1 lg:overflow-y-auto">
          <div className="flex flex-col gap-5 px-page-gutter pb-8 pt-5">
            <Deferred
              promise={deferred.stats}
              height={220}
              title={t('event.details')}
            >
              {(stats) => <EventTrends stats={stats} />}
            </Deferred>

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
              userReports={userReports}
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

type Deferreds = {
  stats: Promise<Result<IssueStatsResponse | null, RustrakError>>;
  aggregates: Promise<Result<IssueAggregates | null, RustrakError>>;
  activity: Promise<Result<ActivityEntry[], RustrakError>>;
  code: Promise<Result<IssueGitHub | null, RustrakError>>;
  members: Promise<Result<ProjectMemberResponse[], RustrakError>>;
};

/**
 * The rail's four reads as one, so it fills in at once rather than block by
 * block. Memoised per loader result: `use` needs the same promise on every
 * render.
 */
const railReads = new WeakMap<Deferreds, ReturnType<typeof readRail>>();
function railData(deferred: Deferreds) {
  let pending = railReads.get(deferred);
  if (!pending) {
    pending = readRail(deferred);
    railReads.set(deferred, pending);
  }
  return pending;
}

async function readRail(deferred: Deferreds) {
  const all = await loadAll([
    deferred.aggregates,
    deferred.activity,
    deferred.code,
    deferred.members,
  ]);
  if (!all.success) return all;
  const [aggregates, activity, code, members] = all.data;
  return Ok({ aggregates, activity, code, members });
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
