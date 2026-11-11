import { issueQueries } from '@/features/issue/api/queries';
import { projectQueries } from '@/features/project/api/queries';
import { releaseQueries } from '@/features/release/api/queries';
import { transactionQueries } from '@/features/transaction/api/queries';
import {
  type OverviewPeriod,
  overviewInterval,
  periodHours,
} from '@/shared/lib/period';

export interface OverviewFilters {
  projectId: number;
  /** Undefined is all time. */
  period?: OverviewPeriod;
  environment?: string;
  release?: string;
}

const ISSUE_VIEWS = {
  recent: { sort: 'last_seen' },
  new: { sort: 'digest_order' },
  frequent: { sort: 'event_count' },
} as const;

export type IssueView = keyof typeof ISSUE_VIEWS;

/**
 * The overview's reads for a URL, built once for the loader and the panels so
 * both name the same cache entries.
 */
export function overviewReads(filters: OverviewFilters, compare: boolean) {
  const { projectId, period, environment, release } = filters;
  const scope = { period, environment, release };
  const interval = overviewInterval(period);
  return {
    summary: projectQueries.summary(projectId, scope),
    // The current series is always its own read, so turning comparison on
    // never changes it; the window before comes from twice the window.
    events: projectQueries.eventTimeseries(projectId, { ...scope, interval }),
    previousEvents:
      compare && period
        ? projectQueries.eventTimeseries(projectId, {
            ...scope,
            period: `${periodHours(period) * 2}h`,
            interval,
          })
        : null,
    sessionSummary: releaseQueries.sessionSummary(projectId, scope),
    sessionTrend: releaseQueries.sessionTimeseries(projectId, {
      ...scope,
      interval,
    }),
    issues: (view: IssueView) =>
      issueQueries.list(projectId, {
        filter: 'open',
        page: 1,
        per_page: 6,
        sort: ISSUE_VIEWS[view].sort,
        order: 'desc',
        environment,
        release,
      }),
    // Transaction stats have no window of their own yet, so this panel is all
    // time whatever the filters say -- said in its description rather than
    // left to look like it follows them.
    latency: transactionQueries.stats(projectId, { page: 1, per_page: 20 }),
    environments: projectQueries.environments(projectId),
    releases: projectQueries.releaseVersions(projectId),
  };
}

export type OverviewReads = ReturnType<typeof overviewReads>;

const HELD = { enabled: false, subscribed: false } as const;

/**
 * The same reads, never asked for: the skeleton draws each panel's
 * placeholder from them. An observer that unmounts mid-flight cancels the
 * loader's request, and the page would ask again.
 */
export function heldReads(reads: OverviewReads): OverviewReads {
  return {
    summary: { ...reads.summary, ...HELD },
    events: { ...reads.events, ...HELD },
    previousEvents: reads.previousEvents && {
      ...reads.previousEvents,
      ...HELD,
    },
    sessionSummary: { ...reads.sessionSummary, ...HELD },
    sessionTrend: { ...reads.sessionTrend, ...HELD },
    issues: (view) => ({ ...reads.issues(view), ...HELD }),
    latency: { ...reads.latency, ...HELD },
    environments: { ...reads.environments, ...HELD },
    releases: { ...reads.releases, ...HELD },
  };
}
