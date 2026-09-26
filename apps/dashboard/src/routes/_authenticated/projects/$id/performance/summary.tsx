import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, Link, notFound } from '@tanstack/react-router';
import { ArrowLeft } from 'lucide-react';
import { useFormatter, useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import { transactionQueries } from '@/features/transaction/api/queries';
import { TransactionsList } from '@/features/transaction/ui/components/transactions-list';
import { translator } from '@/shared/i18n/intl';
import { combine, loadAll } from '@/shared/lib/results';
import { searchPage, searchString } from '@/shared/lib/search-params';
import { LoadFailure } from '@/shared/ui/components/load-failure';
import { Badge } from '@/shared/ui/components/shadcn/badge';

function samplesQuery(
  projectId: number,
  name: string,
  op: string | undefined,
  page: number,
) {
  return transactionQueries.list(projectId, { page, per_page: 20, name, op });
}

export const Route = createFileRoute(
  '/_authenticated/projects/$id/performance/summary',
)({
  validateSearch: (
    search: Record<string, unknown>,
  ): {
    name?: string;
    op?: string;
    page?: number;
  } => ({
    name: searchString(search.name),
    op: searchString(search.op),
    page: searchPage(search.page),
  }),
  loaderDeps: ({ search }) => ({
    name: search.name,
    op: search.op,
    page: search.page ?? 1,
  }),
  loader: ({ params: { id }, deps, context: { queryClient } }) => {
    // The transaction name *is* the address of this page: without it there is
    // no group to summarise, which is a wrong address rather than an outage.
    if (!deps.name) throw notFound();

    // Direct group lookup — correct regardless of how many groups exist (no
    // "fetch page 1 and hope the group is on it").
    // `getTransactionStatForGroup` already turns "this group has no rows" into
    // a successful `null`, so a failure here is a real one.
    return loadAll([
      queryClient.ensureQueryData(projectQueries.detail(id)),
      queryClient.ensureQueryData(
        samplesQuery(id, deps.name, deps.op, deps.page),
      ),
      queryClient.ensureQueryData(
        transactionQueries.groupStat(id, deps.name, deps.op),
      ),
    ]);
  },
  head: ({ match }) => {
    const t = translator('projectPages');
    return {
      meta: [
        {
          title: t('summary.meta.title', {
            name: match.search.name ?? t('summary.meta.fallbackName'),
          }),
        },
      ],
    };
  },
  component: TransactionSummaryPage,
});

function formatMs(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)}ms`;
  return `${(ms / 1000).toFixed(2)}s`;
}

function TransactionSummaryPage() {
  const t = useTranslations('projectPages');
  const format = useFormatter();
  const { id: projectId } = Route.useParams();
  const { name, op, currentPage } = Route.useSearch({
    select: (search) => ({
      name: search.name,
      op: search.op,
      currentPage: search.page ?? 1,
    }),
  });
  // The loader turned an absent `name` into a not-found before this renders.
  const groupName = name ?? '';
  const loaded = combine([
    useSuspenseQuery(projectQueries.detail(projectId)).data,
    useSuspenseQuery(samplesQuery(projectId, groupName, op, currentPage)).data,
    useSuspenseQuery(transactionQueries.groupStat(projectId, groupName, op))
      .data,
  ]);

  if (!loaded.success) {
    return (
      <LoadFailure error={loaded.error} title={t('transaction.loadFailed')} />
    );
  }

  const [, samples, group] = loaded.data;

  const metrics: { id: string; label: string; value: string }[] = group
    ? [
        {
          id: 'count',
          label: t('summary.metricCount'),
          value: format.number(group.count),
        },
        {
          id: 'p50',
          label: t('summary.metricP50'),
          value: formatMs(group.p50_ms),
        },
        {
          id: 'p95',
          label: t('summary.metricP95'),
          value: formatMs(group.p95_ms),
        },
        {
          id: 'p99',
          label: t('summary.metricP99'),
          value: formatMs(group.p99_ms),
        },
        {
          id: 'failureRate',
          label: t('summary.metricFailureRate'),
          value: `${(group.failure_rate * 100).toFixed(1)}%`,
        },
      ]
    : [];

  return (
    <div className="flex flex-col h-[calc(100vh-64px)]">
      <div className="shrink-0 w-full px-4 md:px-8 py-4 md:py-6 border-b">
        <Link
          to="/projects/$id/performance"
          params={{ id: projectId }}
          className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors mb-3"
        >
          <ArrowLeft className="size-4" />
          {t('performance.backLink')}
        </Link>
        <div className="flex items-center gap-2 flex-wrap">
          <h1 className="font-mono text-lg font-semibold break-all">{name}</h1>
          {op && <Badge variant="secondary">{op}</Badge>}
        </div>

        {metrics.length > 0 && (
          <dl className="mt-3 flex flex-wrap gap-x-8 gap-y-2">
            {metrics.map((m) => (
              <div key={m.id}>
                <dt className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
                  {m.label}
                </dt>
                <dd className="font-mono text-sm font-semibold tabular-nums">
                  {m.value}
                </dd>
              </div>
            ))}
          </dl>
        )}
      </div>

      <div className="flex-1 overflow-hidden w-full px-4 md:px-8 py-4 md:py-6 flex flex-col gap-3">
        <h2 className="shrink-0 text-xs font-bold uppercase tracking-widest text-muted-foreground">
          {t('summary.samples')}
        </h2>
        <TransactionsList
          projectId={projectId}
          initialTransactions={samples}
          currentPage={currentPage}
        />
      </div>
    </div>
  );
}
