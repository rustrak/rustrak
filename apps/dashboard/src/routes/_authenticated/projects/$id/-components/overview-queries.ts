import type { QueryClient } from '@tanstack/react-query';
import { issueQueries } from '@/features/issue/api/queries';
import { projectQueries } from '@/features/project/api/queries';
import { releaseQueries } from '@/features/release/api/queries';
import {
  type OverviewPeriod,
  overviewInterval,
} from '@/features/release/model/session-health';
import { transactionQueries } from '@/features/transaction/api/queries';

/**
 * The overview's reads, apart from the tiles that draw them.
 *
 * A route's loader is not code-split, so whatever it imports ships with the
 * entry. Kept here, the loader can start the tiles' requests without pulling
 * their charts into every page's first payload.
 */
export interface TileProps {
  projectId: number;
  period?: OverviewPeriod;
}

export const tileQueries = {
  errorVolume: ({ projectId, period }: TileProps) =>
    projectQueries.timeseries(projectId, period, overviewInterval(period)),
  counters: ({ projectId, period }: TileProps) =>
    projectQueries.summary(projectId, period),
  sessionSummary: ({ projectId, period }: TileProps) =>
    releaseQueries.sessionSummary(projectId, period),
  sessionTimeseries: ({ projectId, period }: TileProps) =>
    releaseQueries.sessionTimeseries(
      projectId,
      period,
      overviewInterval(period),
    ),
  // Transaction stats have no period filter of their own yet, so this tile is
  // all-time regardless of the selected window. Said out loud in the subtitle
  // rather than silently pretending to follow the filter.
  performance: ({ projectId }: TileProps) =>
    transactionQueries.stats(projectId, { page: 1, per_page: 20 }),
  // The issues endpoint takes no time window, and `event_count` is the issue's
  // lifetime total, so this ranking is all-time whatever the page filter says.
  // Labelled rather than left to look like it follows the filter, the same way
  // the latency tile is.
  topIssues: ({ projectId }: TileProps) =>
    issueQueries.list(projectId, {
      filter: 'open',
      page: 1,
      per_page: 5,
      sort: 'event_count',
      order: 'desc',
    }),
};

/**
 * Every tile's read, for the loader to start alongside the page's own. Not
 * awaited there: each tile still appears when its own answer lands.
 */
export function prefetchOverviewTiles(
  queryClient: QueryClient,
  tile: TileProps,
): void {
  void queryClient.prefetchQuery(tileQueries.errorVolume(tile));
  void queryClient.prefetchQuery(tileQueries.counters(tile));
  void queryClient.prefetchQuery(tileQueries.sessionSummary(tile));
  void queryClient.prefetchQuery(tileQueries.sessionTimeseries(tile));
  void queryClient.prefetchQuery(tileQueries.performance(tile));
  void queryClient.prefetchQuery(tileQueries.topIssues(tile));
}
