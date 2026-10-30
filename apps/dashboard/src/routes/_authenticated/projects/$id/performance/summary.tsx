import { noop, useQuery } from '@tanstack/react-query';
import { createFileRoute, notFound } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import {
  DEFAULT_PERIOD,
  PERIOD_CHOICES,
  type PeriodChoice,
} from '@/features/release/model/session-health';
import { PeriodFilter } from '@/features/release/ui/components/period-filter';
import { transactionQueries } from '@/features/transaction/api/queries';
import { windowHours } from '@/features/transaction/model/endpoints';
import {
  CallsChart,
  EndpointFacts,
  EndpointFigures,
  LatencyChart,
  SlowestSamples,
} from '@/features/transaction/ui/components/endpoint-detail';
import { searchOneOf, searchString } from '@/shared/lib/search-params';
import { Panel, RailBlock } from '@/shared/ui/components/panel';
import { QueryState } from '@/shared/ui/components/query-state';
import { usePageTrail } from '@/shared/ui/hooks/use-page-trail';

interface EndpointSearch {
  name?: string;
  op?: string;
  period?: PeriodChoice;
  environment?: string;
}

/**
 * The page's three reads for a URL, built once for the loader and the
 * components so both name the same cache entries.
 */
function endpointReads(projectId: number, search: EndpointSearch) {
  const period = search.period ?? DEFAULT_PERIOD;
  const group = {
    name: search.name ?? '',
    op: search.op,
    period: period === 'all' ? undefined : period,
    environment: search.environment,
  };
  return {
    stats: transactionQueries.group(projectId, group),
    trend: transactionQueries.groupTrend(projectId, group),
    samples: transactionQueries.list(projectId, {
      ...group,
      sort: 'slowest',
      per_page: 10,
    }),
  };
}

/**
 * One endpoint, laid out as the release page is: its name and window in the
 * header, how it is doing and how it moved in the reading column, the facts
 * that describe it in the rail. A row of the performance list opens it,
 * keeping the window and environment it was read in.
 */
export const Route = createFileRoute(
  '/_authenticated/projects/$id/performance/summary',
)({
  validateSearch: (search: Record<string, unknown>): EndpointSearch => ({
    name: searchString(search.name),
    op: searchString(search.op),
    period: searchOneOf(search.period, PERIOD_CHOICES),
    environment: searchString(search.environment),
  }),
  // The name *is* the address: without it there is no endpoint to show.
  beforeLoad: ({ search }) => {
    if (!search.name) throw notFound();
  },
  loaderDeps: ({ search }) => search,
  // Three panels, each started here and landing on its own. Nothing
  // suspends: the header is the address's, and each panel draws its own
  // placeholder, so the page needs no skeleton of its own.
  loader: ({ context: { queryClient }, params, deps }) => {
    const reads = endpointReads(Number.parseInt(params.id, 10), deps);
    void queryClient.query(reads.stats).catch(noop);
    void queryClient.query(reads.trend).catch(noop);
    void queryClient.query(reads.samples).catch(noop);
  },
  component: EndpointPage,
});

function EndpointPage() {
  const t = useTranslations('performance.detail');
  const tPages = useTranslations('projectPages');
  const { id } = Route.useParams();
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const projectId = Number.parseInt(id, 10);
  const period = search.period ?? DEFAULT_PERIOD;
  const name = search.name ?? '';
  const op = search.op ?? null;
  usePageTrail([{ label: name, mono: true }]);

  const reads = endpointReads(projectId, search);
  const stats = useQuery(reads.stats);
  const trend = useQuery(reads.trend);
  const samples = useQuery(reads.samples);

  return (
    <div className="flex h-full flex-col">
      <title>
        {tPages('summary.meta.title', {
          name: search.name ?? tPages('summary.meta.fallbackName'),
        })}
      </title>
      <header className="@container shrink-0 border-border-divider border-b px-page-gutter pt-6 pb-5">
        <div className="flex flex-col gap-4 @4xl:flex-row @4xl:items-start @4xl:justify-between @4xl:gap-8">
          <div className="flex min-w-0 flex-col gap-1">
            {op ? <span className="text-fg-meta text-meta">{op}</span> : null}
            <h1 className="break-all font-mono text-fg text-page-title">
              {name}
            </h1>
          </div>
          <PeriodFilter
            period={period}
            onChange={(next) =>
              navigate({
                search: (prev) => ({
                  ...prev,
                  period: next === DEFAULT_PERIOD ? undefined : next,
                }),
                replace: true,
              })
            }
          />
        </div>
      </header>

      <div className="flex min-h-0 flex-1 max-lg:flex-col max-lg:overflow-y-auto">
        <main className="min-w-0 flex-1 lg:overflow-y-auto">
          <div className="flex flex-col gap-5 px-page-gutter pt-5 pb-8">
            <QueryState
              query={stats}
              placeholder={<Placeholder height={104} />}
              title={t('figures')}
            >
              {(group) =>
                group ? (
                  <EndpointFigures
                    group={group}
                    windowHours={windowHours(period)}
                  />
                ) : (
                  <p className="text-fg-subtle text-meta">{t('noSamples')}</p>
                )
              }
            </QueryState>
            <Panel title={t('latency')} hint={t('latencyHint')}>
              <QueryState
                query={trend}
                placeholder={<Placeholder height={220} />}
                title={t('latency')}
              >
                {(points) => <LatencyChart points={points} />}
              </QueryState>
            </Panel>
            <Panel title={t('calls')} hint={t('callsHint')}>
              <QueryState
                query={trend}
                placeholder={<Placeholder height={220} />}
                title={t('calls')}
              >
                {(points) => <CallsChart points={points} />}
              </QueryState>
            </Panel>
            <Panel flush title={t('slowest')} hint={t('slowestHint')}>
              <QueryState
                query={samples}
                placeholder={<Placeholder height={240} />}
                title={t('slowest')}
              >
                {(page) => (
                  <SlowestSamples projectId={projectId} rows={page.items} />
                )}
              </QueryState>
            </Panel>
          </div>
        </main>
        <aside className="border-border-divider bg-panel max-lg:border-t lg:w-80 lg:shrink-0 lg:overflow-y-auto lg:border-l xl:w-88">
          <RailBlock title={t('details')}>
            <QueryState
              query={stats}
              placeholder={<Placeholder height={160} />}
              title={t('details')}
            >
              {(group) =>
                group ? (
                  <EndpointFacts
                    group={group}
                    op={op}
                    environment={search.environment}
                  />
                ) : null
              }
            </QueryState>
          </RailBlock>
        </aside>
      </div>
    </div>
  );
}

function Placeholder({ height }: { height: number }) {
  return (
    <div
      className="animate-pulse rounded-lg bg-surface-hover motion-reduce:animate-none"
      style={{ height }}
    />
  );
}
