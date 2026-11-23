/**
 * Reads for web vitals: a project's or a page's score, its trend, and the
 * pages ranked by what they cost it.
 */
import type {
  WebVitalsGetBreakdownData,
  WebVitalsGetSummaryData,
  WebVitalsListPagesData,
} from '@rustrak/client';
import { queryOptions } from '@tanstack/react-query';
import { projectScope } from '@/shared/api/keys';
import { orThrow } from '@/shared/api/query';
import { api } from '@/shared/api/rustrak';

type WebVitalsQuery = NonNullable<WebVitalsGetSummaryData['query']>;
type WebVitalsPagesQuery = NonNullable<WebVitalsListPagesData['query']>;
type WebVitalsBreakdownQuery = NonNullable<WebVitalsGetBreakdownData['query']>;

export const webVitalQueries = {
  all: (projectId: number) =>
    [...projectScope(projectId), 'web-vitals'] as const,
  summary: (projectId: number, query: WebVitalsQuery) =>
    queryOptions({
      queryKey: [...webVitalQueries.all(projectId), 'summary', query],
      queryFn: ({ signal }) =>
        orThrow(
          api().webVitals.getSummary({
            path: { project_id: projectId },
            query,
            signal,
          }),
        ),
    }),
  trend: (projectId: number, query: WebVitalsQuery) =>
    queryOptions({
      queryKey: [...webVitalQueries.all(projectId), 'trend', query],
      queryFn: ({ signal }) =>
        orThrow(
          api().webVitals.getTimeseries({
            path: { project_id: projectId },
            query,
            signal,
          }),
        ),
    }),
  /** What one vital was about, the element losing the most score first. */
  targets: (projectId: number, query: WebVitalsQuery) =>
    queryOptions({
      queryKey: [...webVitalQueries.all(projectId), 'targets', query],
      queryFn: ({ signal }) =>
        orThrow(
          api().webVitals.listTargets({
            path: { project_id: projectId },
            query,
            signal,
          }),
        ),
    }),
  /** One vital on each device or browser. */
  breakdown: (projectId: number, query: WebVitalsBreakdownQuery) =>
    queryOptions({
      queryKey: [...webVitalQueries.all(projectId), 'breakdown', query],
      queryFn: ({ signal }) =>
        orThrow(
          api().webVitals.getBreakdown({
            path: { project_id: projectId },
            query,
            signal,
          }),
        ),
    }),
  /** The worst measurements, with where each happened. */
  samples: (projectId: number, query: WebVitalsQuery) =>
    queryOptions({
      queryKey: [...webVitalQueries.all(projectId), 'samples', query],
      queryFn: ({ signal }) =>
        orThrow(
          api().webVitals.listSamples({
            path: { project_id: projectId },
            query,
            signal,
          }),
        ),
    }),
  pages: (projectId: number, query: WebVitalsPagesQuery) =>
    queryOptions({
      queryKey: [...webVitalQueries.all(projectId), 'pages', query],
      queryFn: ({ signal }) =>
        orThrow(
          api().webVitals.listPages({
            path: { project_id: projectId },
            query,
            signal,
          }),
        ),
    }),
};
