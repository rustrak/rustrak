import type { TransactionDetailResponse } from '@rustrak/client';
import {
  CopyIcon,
  cn,
  formatSpanDuration,
  Skeleton,
  Tag,
  Waterfall,
} from '@rustrak/ui';
import { noop, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { type ReactNode, useState } from 'react';
import { toast } from 'sonner';
import { useFormatter, useTranslations } from 'use-intl';
import { transactionQueries } from '@/features/transaction/api/queries';
import {
  type MeasurementRow,
  readMeasurements,
  readTransactionPayload,
  type WaterfallRow,
  waterfallSpans,
} from '@/features/transaction/lib/transaction-payload';
import { endpointHref } from '@/features/transaction/model/endpoints';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { usePageTrail } from '@/shared/ui/hooks/use-page-trail';

export const Route = createFileRoute(
  '/_authenticated/projects/$id/performance/$txnId',
)({
  // The project is the layout's; the transaction is the page.
  loader: ({ context: { queryClient }, params }) => {
    void queryClient
      .query(
        transactionQueries.detail(Number.parseInt(params.id, 10), params.txnId),
      )
      .catch(noop);
  },
  pendingComponent: TransactionLoading,
  component: TransactionPage,
});

const RATING_TEXT = {
  good: 'text-success-fg',
  meh: 'text-sev-warning-fg',
  poor: 'text-sev-error-fg',
} as const;

/**
 * One transaction, laid out as the issue page is: what it was and how it
 * ended in the header, its trace in the reading column, the request, tags
 * and user it carried in the rail. The breadcrumbs lead back to its
 * endpoint and to the list.
 */
function TransactionPage() {
  const t = useTranslations('performance.transaction');
  const tPages = useTranslations('projectPages');
  const { id, txnId } = Route.useParams();
  const projectId = Number.parseInt(id, 10);
  const { data: txn } = useSuspenseQuery(
    transactionQueries.detail(projectId, txnId),
  );
  const name = txn.transaction_name || t('unnamed');
  const payload = readTransactionPayload(txn);

  usePageTrail([
    {
      label: name,
      mono: true,
      href: endpointHref(projectId, { name, op: payload?.op ?? null }),
    },
    { label: txn.event_id.slice(0, 8), mono: true },
  ]);

  if (!payload) return null;

  const rows = waterfallSpans(payload, name);
  const measurements = readMeasurements(payload.measurements);

  return (
    <div className="flex h-full flex-col max-lg:overflow-y-auto">
      <title>
        {tPages('transaction.meta.title', { name: txn.transaction_name })}
      </title>
      <TransactionHeader
        txn={txn}
        name={name}
        op={payload.op}
        status={payload.status}
      />
      <div className="flex min-h-0 flex-1 max-lg:flex-col">
        <main className="min-w-0 flex-1 lg:overflow-y-auto">
          <div className="flex flex-col gap-5 px-page-gutter pt-5 pb-8">
            {measurements.length > 0 ? <Vitals rows={measurements} /> : null}
            <Trace rows={rows} name={name} />
          </div>
        </main>
        <aside className="border-border-divider bg-panel max-lg:border-t lg:w-80 lg:shrink-0 lg:overflow-y-auto lg:border-l xl:w-88">
          <RailBlock title={t('request')} data={payload.request} />
          <RailBlock title={t('tags')} data={payload.tags} />
          <RailBlock title={t('user')} data={payload.user} />
        </aside>
      </div>
    </div>
  );
}

/** The transaction's header, trace and rail, with what they hold to come. */
function TransactionLoading() {
  const t = useTranslations('performance.transaction');
  const tPages = useTranslations('projectPages');
  return (
    <PageLoading>
      <div className="flex h-full flex-col max-lg:overflow-y-auto">
        <title>{tPages('transaction.meta.fallbackTitle')}</title>
        <header className="shrink-0 border-border-divider border-b px-page-gutter pt-6 pb-5">
          <div className="flex flex-col gap-2">
            <Skeleton className="h-4 w-40" />
            <Skeleton className="h-8 w-96 max-w-full" />
            <Skeleton className="mt-1 h-4 w-64" />
          </div>
        </header>
        <div className="flex min-h-0 flex-1 max-lg:flex-col">
          <main className="min-w-0 flex-1 lg:overflow-y-auto">
            <div className="flex flex-col gap-5 px-page-gutter pt-5 pb-8">
              <section className="flex min-w-0 flex-col gap-3 rounded-2xl border border-border-subtle bg-surface px-5 pt-4 pb-5">
                <h2 className="text-card-title text-fg">{t('trace')}</h2>
                <Skeleton className="h-64 w-full" />
              </section>
            </div>
          </main>
          <aside className="flex flex-col gap-3 border-border-divider bg-panel px-5 py-4 max-lg:border-t lg:w-80 lg:shrink-0 lg:border-l xl:w-88">
            <Skeleton className="h-4 w-24" />
            <Skeleton className="h-32 w-full" />
          </aside>
        </div>
      </div>
    </PageLoading>
  );
}

/** What ran, how long it took, how it ended, where and when. */
function TransactionHeader({
  txn,
  name,
  op,
  status,
}: {
  txn: TransactionDetailResponse;
  name: string;
  op: string | undefined;
  status: string | undefined;
}) {
  const t = useTranslations('performance.transaction');
  const format = useFormatter();
  const failed = status != null && status !== 'ok';
  const copyId = () =>
    navigator.clipboard
      ?.writeText(txn.event_id)
      .then(() => toast.success(t('idCopied')))
      .catch(() => {});

  return (
    <header className="shrink-0 border-border-divider border-b px-page-gutter pt-6 pb-5">
      <div className="flex flex-col gap-1">
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            onClick={copyId}
            title={t('copyId')}
            className="group/id inline-flex items-center gap-1.5 rounded-sm font-mono text-fg-meta text-mono-sm hover:text-fg"
          >
            {txn.event_id.slice(0, 8)}
            <CopyIcon
              size="sm"
              aria-hidden="true"
              className="opacity-0 group-hover/id:opacity-100"
            />
          </button>
          {status ? (
            <Tag variant="soft" tone={failed ? 'error' : 'neutral'}>
              {status}
            </Tag>
          ) : null}
          {op ? <span className="text-fg-meta text-meta">{op}</span> : null}
        </div>
        <h1 className="break-all font-mono text-fg text-page-title">{name}</h1>
        <p className="mt-1 flex flex-wrap items-center gap-x-4 gap-y-0.5 text-fg-subtle text-meta">
          <span className="font-mono text-fg-secondary">
            {txn.duration_ms == null
              ? '–'
              : formatSpanDuration(txn.duration_ms)}
          </span>
          <span title={format.dateTime(new Date(txn.timestamp), 'precise')}>
            {format.relativeTime(new Date(txn.timestamp))}
          </span>
          {txn.environment ? <span>{txn.environment}</span> : null}
          {txn.release ? (
            <span className="font-mono">{txn.release}</span>
          ) : null}
          {txn.platform ? <span>{txn.platform}</span> : null}
        </p>
      </div>
    </header>
  );
}

/** The measurements in one strip, the web vitals rated by web.dev's lines. */
function Vitals({ rows }: { rows: MeasurementRow[] }) {
  const t = useTranslations('transactions.measurements');
  return (
    <dl className="grid grid-cols-2 gap-px overflow-hidden rounded-2xl border border-border-subtle bg-border-subtle sm:grid-cols-3 xl:grid-cols-6">
      {rows.map((row) => (
        <div
          key={row.key}
          className="flex min-w-0 flex-col gap-1.5 bg-surface px-4 py-3.5"
        >
          <dt className="truncate text-fg-meta text-meta">
            {row.vital ? t(row.key.toLowerCase() as 'lcp') : row.key}
          </dt>
          <dd
            className={cn(
              'text-title tabular-nums',
              row.rating ? RATING_TEXT[row.rating] : 'text-fg',
            )}
          >
            {measurement(row)}
          </dd>
        </div>
      ))}
    </dl>
  );
}

function measurement(row: MeasurementRow): string {
  if (row.unit === 'millisecond' || row.unit == null || row.unit === '') {
    return row.key.toLowerCase() === 'cls'
      ? row.value.toFixed(3)
      : formatSpanDuration(row.value);
  }
  if (row.unit === 'second') return formatSpanDuration(row.value * 1000);
  if (row.unit === 'ratio' || row.unit === 'none') return row.value.toFixed(3);
  return `${row.value} ${row.unit}`;
}

/** The trace, the selected span's facts opening under its row. */
function Trace({ rows, name }: { rows: WaterfallRow[]; name: string }) {
  const t = useTranslations('performance.transaction');
  const [selected, setSelected] = useState<string | null>(null);
  return (
    <section className="flex min-w-0 flex-col rounded-2xl border border-border-subtle bg-surface">
      <header className="flex items-baseline justify-between gap-3 px-5 pt-4 pb-3">
        <h2 className="text-card-title text-fg">{t('trace')}</h2>
        <span className="text-fg-subtle text-meta">
          {t('spans', { count: rows.length - 1 })}
        </span>
      </header>
      {rows.length > 1 ? (
        <Waterfall
          spans={rows}
          selectedId={selected}
          onSelect={(span) => setSelected(span?.id ?? null)}
          renderDetail={(span) => {
            // The waterfall hands back its own span type; ours has self time.
            const row = rows.find((r) => r.id === span.id);
            return row ? <SpanFacts span={row} /> : null;
          }}
          showMissingInstrumentation
          label={t('traceOf', { name })}
        />
      ) : (
        <p className="px-5 pb-5 text-fg-subtle text-meta">{t('noSpans')}</p>
      )}
    </section>
  );
}

function SpanFacts({ span }: { span: WaterfallRow }) {
  const t = useTranslations('performance.transaction');
  const facts: [string, ReactNode][] = [
    [t('op'), span.op],
    [t('description'), span.description],
    [t('duration'), formatSpanDuration(span.endMs - span.startMs)],
    [t('selfTime'), formatSpanDuration(span.selfMs)],
    [t('start'), `+${formatSpanDuration(span.startMs)}`],
    [t('status'), span.status],
    [t('spanId'), span.id],
  ];
  return (
    <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1.5 text-meta">
      {facts
        .filter(([, value]) => value)
        .map(([label, value]) => (
          <div key={label} className="contents">
            <dt className="text-fg-subtle">{label}</dt>
            <dd className="min-w-0 break-all font-mono text-fg-secondary text-mono-sm">
              {value}
            </dd>
          </div>
        ))}
    </dl>
  );
}

/** A block of the rail: the scalar fields of one part of the payload. */
function RailBlock({
  title,
  data,
}: {
  title: string;
  data: Record<string, unknown> | null;
}) {
  const entries = Object.entries(data ?? {}).filter(
    ([, value]) => value != null && typeof value !== 'object',
  );
  if (entries.length === 0) return null;
  return (
    <section className="border-border-divider border-b px-5 py-4 last:border-b-0">
      <h2 className="mb-3 font-mono text-column text-fg-meta uppercase">
        {title}
      </h2>
      <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 text-meta">
        {entries.map(([key, value]) => (
          <div key={key} className="contents">
            <dt className="text-fg-subtle">{key}</dt>
            <dd className="min-w-0 break-all text-right font-mono text-fg-secondary text-mono-sm">
              {String(value)}
            </dd>
          </div>
        ))}
      </dl>
    </section>
  );
}
