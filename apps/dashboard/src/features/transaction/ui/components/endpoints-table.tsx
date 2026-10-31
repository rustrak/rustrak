import type {
  TransactionStatsResponse,
  TransactionsGetStatsResponse,
} from '@rustrak/client';
import {
  createDataTableColumnHelper,
  DataTable,
  DataTableColumnsButton,
  type DataTableQuery,
  EnvironmentIcon,
  formatSpanDuration,
  QueryBar,
  type QueryField,
  useDataTable,
} from '@rustrak/ui';
import { type ReactNode, useMemo } from 'react';
import { useTranslations } from 'use-intl';
import { failureTone } from '@/features/transaction/model/endpoints';
import { RelativeTime } from '@/shared/ui/components/relative-time';
import { useColumnFormatter } from '@/shared/ui/hooks/use-column-formatter';

const column = createDataTableColumnHelper<TransactionStatsResponse>();

/** One empty list, so the table's data is stable while the first page loads. */
const NO_ENDPOINTS: TransactionStatsResponse[] = [];

/** The id a row is known by: its name and op, which make the endpoint. */
function endpointId(row: { transaction_name: string; op: string | null }) {
  return `${row.op ?? ''} ${row.transaction_name}`;
}

const FAILURE_TEXT = {
  warning: 'text-sev-warning-fg',
  bad: 'text-sev-error-fg',
} as const;

/**
 * The endpoints, ranked by the time they take in all. The first column says
 * which one, the second how much of the time it takes -- the column the
 * ranking is about, so it carries the one bar -- and the rest are figures in
 * one weight, the failure rate coloured only when it needs a look.
 */
export function EndpointsTable({
  page,
  query,
  onQueryChange,
  loading,
  onOpen,
  environments,
  period,
}: {
  /** Absent while the first page loads: the table draws skeleton rows. */
  page?: TransactionsGetStatsResponse;
  query: DataTableQuery;
  onQueryChange: (
    updater: (previous: DataTableQuery) => DataTableQuery,
  ) => void;
  loading: boolean;
  onOpen: (row: TransactionStatsResponse) => void;
  /** For the query bar's `environment:` suggestions. */
  environments: string[];
  /** The window, beside the query bar. */
  period: ReactNode;
}) {
  const t = useTranslations('performance');
  const format = useColumnFormatter();

  const columns = useMemo(() => {
    const duration = (ms: number) => (
      <span className="text-fg-secondary tabular-nums">
        {formatSpanDuration(ms)}
      </span>
    );
    const figure = { align: 'end' as const };
    // The server ranks from the most down; both directions read the same.
    const desc = { asc: t('table.most'), desc: t('table.most') };

    return column.columns([
      column.accessor('transaction_name', {
        id: 'endpoint',
        header: t('columns.endpoint'),
        enableHiding: false,
        enableSorting: false,
        cell: ({ row }) => (
          <span className="flex min-w-0 items-baseline gap-2">
            <span
              className="truncate font-mono text-fg text-mono"
              title={row.original.transaction_name}
            >
              {row.original.transaction_name}
            </span>
            {row.original.op ? (
              <span className="shrink-0 text-fg-subtle text-meta">
                {row.original.op}
              </span>
            ) : null}
          </span>
        ),
        meta: { label: t('columns.endpoint') },
      }),
      column.accessor('time_share', {
        id: 'time_spent',
        header: t('columns.timeSpent'),
        sortDescFirst: true,
        cell: ({ getValue }) => (
          <span className="flex w-full items-center gap-2">
            <span className="h-1.5 min-w-0 flex-1 overflow-hidden rounded-pill bg-surface-chip">
              <span
                className="block h-full rounded-pill bg-chart-1"
                style={{ width: `${Math.max(getValue() * 100, 1.5)}%` }}
              />
            </span>
            <span className="w-11 shrink-0 text-right text-fg-secondary text-meta tabular-nums">
              {format.number(getValue(), {
                style: 'percent',
                maximumFractionDigits: getValue() < 0.1 ? 1 : 0,
              })}
            </span>
          </span>
        ),
        meta: {
          label: t('columns.timeSpent'),
          width: 168,
          sortLabels: desc,
        },
      }),
      column.accessor('count', {
        id: 'count',
        header: t('columns.calls'),
        sortDescFirst: true,
        cell: ({ getValue }) => (
          <span className="text-fg-secondary tabular-nums">
            {format.number(getValue(), 'compact')}
          </span>
        ),
        meta: {
          label: t('columns.calls'),
          width: 96,
          sortLabels: desc,
          ...figure,
        },
      }),
      column.accessor('avg_ms', {
        id: 'avg',
        header: t('columns.avg'),
        sortDescFirst: true,
        cell: ({ getValue }) => duration(getValue()),
        meta: {
          label: t('columns.avg'),
          width: 92,
          sortLabels: desc,
          ...figure,
        },
      }),
      column.accessor('p50_ms', {
        id: 'p50',
        header: 'p50',
        enableSorting: false,
        cell: ({ getValue }) => duration(getValue()),
        meta: { label: 'p50', width: 88, ...figure },
      }),
      column.accessor('p95_ms', {
        id: 'p95',
        header: 'p95',
        enableSorting: false,
        cell: ({ getValue }) => duration(getValue()),
        meta: { label: 'p95', width: 88, ...figure },
      }),
      column.accessor('failure_rate', {
        id: 'failure_rate',
        header: t('columns.failures'),
        sortDescFirst: true,
        cell: ({ getValue }) => {
          const rate = getValue();
          const tone = failureTone(rate);
          return (
            <span
              className={
                tone ? FAILURE_TEXT[tone] : 'text-fg-ghost tabular-nums'
              }
            >
              {format.number(rate, {
                style: 'percent',
                maximumFractionDigits: 1,
              })}
            </span>
          );
        },
        meta: {
          label: t('columns.failures'),
          width: 96,
          sortLabels: desc,
          ...figure,
        },
      }),
      column.accessor('last_seen', {
        id: 'last_seen',
        header: t('columns.lastSeen'),
        enableSorting: false,
        cell: ({ getValue }) => (
          <span
            className="text-fg-secondary"
            title={format.dateTime(new Date(getValue()), 'dateTime')}
          >
            <RelativeTime at={getValue()} />
          </span>
        ),
        meta: { label: t('columns.lastSeen'), width: 128, ...figure },
      }),
    ]);
  }, [t, format]);

  const fields = useMemo<QueryField[]>(
    () => [
      {
        key: 'environment',
        label: t('environment'),
        icon: EnvironmentIcon,
        variant: 'options',
        multiple: false,
        options: environments.map((value) => ({ value, label: value })),
      },
    ],
    [environments, t],
  );

  const table = useDataTable({
    data: page?.items ?? NO_ENDPOINTS,
    columns,
    rowCount: page?.total_count ?? 0,
    query,
    onQueryChange,
    getRowId: endpointId,
    storageKey: 'endpoints',
    initialVisibility: { p50: false, last_seen: false },
  });

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      {/* Narrow, the bar takes a row and the window and columns the next. */}
      <div className="flex flex-wrap items-center gap-2">
        <QueryBar
          className="min-w-0 max-sm:basis-full"
          fields={fields}
          filters={query.filters}
          search={query.search}
          placeholder={t('search')}
          onChange={(next) =>
            onQueryChange((previous) => ({
              ...previous,
              filters: next.filters,
              search: next.search,
              pagination: { ...previous.pagination, pageIndex: 0 },
            }))
          }
        />
        <div className="flex min-w-0 flex-1 items-center gap-2 sm:flex-none">
          {period}
          <DataTableColumnsButton table={table} />
        </div>
      </div>
      <DataTable
        table={table}
        loading={!page || loading}
        className="min-h-0 flex-1"
        onRowClick={(row) => onOpen(row.original)}
        empty={{ title: t('emptyWindow') }}
      />
    </div>
  );
}
