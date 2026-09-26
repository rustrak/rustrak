import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, Link } from '@tanstack/react-router';
import { ArrowLeft } from 'lucide-react';
import { useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import { transactionQueries } from '@/features/transaction/api/queries';
import { readTransactionPayload } from '@/features/transaction/lib/transaction-payload';
import { MeasurementsCard } from '@/features/transaction/ui/components/measurements-card';
import { SpanWaterfall } from '@/features/transaction/ui/components/span-waterfall';
import { translator } from '@/shared/i18n/intl';
import { combine, loadAll } from '@/shared/lib/results';
import { LoadFailure } from '@/shared/ui/components/load-failure';
import { TransactionBadges } from './$txnId/-components/transaction-badges';

export const Route = createFileRoute(
  '/_authenticated/projects/$id/performance/$txnId',
)({
  loader: ({ params: { id, txnId }, context: { queryClient } }) =>
    loadAll([
      queryClient.ensureQueryData(projectQueries.detail(id)),
      queryClient.ensureQueryData(transactionQueries.detail(id, txnId)),
    ]),
  head: ({ loaderData }) => {
    const t = translator('projectPages');
    return {
      meta: [
        {
          title: loaderData?.success
            ? t('transaction.meta.title', {
                name: loaderData.data[1].transaction_name,
              })
            : t('transaction.meta.fallbackTitle'),
        },
      ],
    };
  },
  component: TransactionDetailPage,
});

/** Renders the primitive entries of an object as a key/value list. */
function KeyValuePanel({
  title,
  data,
}: {
  title: string;
  data: Record<string, unknown>;
}) {
  const entries = Object.entries(data).filter(
    ([, v]) => v != null && typeof v !== 'object',
  );
  if (entries.length === 0) return null;

  return (
    <div className="rounded-lg border">
      <div className="border-b px-4 py-2.5 text-xs font-bold uppercase tracking-widest text-muted-foreground">
        {title}
      </div>
      <dl className="divide-y">
        {entries.map(([key, value]) => (
          <div
            key={key}
            className="flex items-start justify-between gap-4 px-4 py-2 text-sm"
          >
            <dt className="font-mono text-muted-foreground">{key}</dt>
            <dd className="font-mono text-right break-all">{String(value)}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

function TransactionDetailPage() {
  const t = useTranslations('projectPages');
  const { id: projectId, txnId } = Route.useParams();
  const loaded = combine([
    useSuspenseQuery(projectQueries.detail(projectId)).data,
    useSuspenseQuery(transactionQueries.detail(projectId, txnId)).data,
  ]);

  if (!loaded.success) {
    return (
      <LoadFailure error={loaded.error} title={t('transaction.loadFailed')} />
    );
  }

  const [, txn] = loaded.data;

  const {
    trace,
    spans,
    measurements,
    tags,
    request,
    user,
    transactionStart,
    transactionEnd,
    op,
    status,
  } = readTransactionPayload(txn);

  return (
    <div className="flex flex-col h-[calc(100vh-64px)]">
      {/* Header */}
      <div className="shrink-0 w-full px-4 md:px-8 py-4 md:py-6 border-b">
        <Link
          to="/projects/$id/performance"
          params={{ id: projectId }}
          className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors mb-3"
        >
          <ArrowLeft className="size-4" />
          {t('performance.backLink')}
        </Link>
        <h1 className="font-mono text-lg font-semibold break-all">
          {txn.transaction_name || t('transaction.unnamed')}
        </h1>
        <TransactionBadges txn={txn} op={op} status={status} />
      </div>

      {/* Body */}
      <div className="flex-1 overflow-auto w-full px-4 md:px-8 py-4 md:py-6 space-y-6">
        {measurements && <MeasurementsCard measurements={measurements} />}

        <section className="rounded-lg border">
          <div className="border-b px-4 py-2.5 flex items-center justify-between">
            <h2 className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
              {t('transaction.spans')}
            </h2>
            <span className="text-xs text-muted-foreground">
              {t('spanCount', { count: spans.length })}
            </span>
          </div>
          <div className="p-3">
            {spans.length === 0 && !trace ? (
              <p className="text-sm text-muted-foreground px-1 py-4 text-center">
                {t('transaction.noSpans')}
              </p>
            ) : (
              <SpanWaterfall
                spans={spans}
                trace={trace}
                transactionStart={transactionStart}
                transactionEnd={transactionEnd}
              />
            )}
          </div>
        </section>

        <div className="grid gap-4 md:grid-cols-2">
          {tags && <KeyValuePanel title={t('transaction.tags')} data={tags} />}
          {request && (
            <KeyValuePanel title={t('transaction.request')} data={request} />
          )}
          {user && <KeyValuePanel title={t('transaction.user')} data={user} />}
        </div>
      </div>
    </div>
  );
}
