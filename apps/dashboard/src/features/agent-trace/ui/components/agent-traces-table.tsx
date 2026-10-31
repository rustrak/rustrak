import type { AgentsTracesResponse, AgentTraceSummary } from '@rustrak/client';
import {
  createDataTableColumnHelper,
  DataTable,
  type DataTableQuery,
  formatSpanDuration,
  QueryBar,
  type QueryField,
  Tag,
  useDataTable,
} from '@rustrak/ui';
import { type ReactNode, useMemo } from 'react';
import { useTranslations } from 'use-intl';
import { RelativeTime } from '@/shared/ui/components/relative-time';
import { useStableFormatter } from '@/shared/ui/hooks/use-stable-formatter';

const column = createDataTableColumnHelper<AgentTraceSummary>();

/** One empty list, so the table's data is stable while the first page loads. */
const NO_TRACES: AgentTraceSummary[] = [];

/**
 * Every agent trace, a row each: which agents ran, then its totals -- how
 * long, how many model and tool calls, how many tokens, how many failed --
 * each a ranking the header sorts by, largest first.
 */
export function AgentTracesTable({
  page,
  query,
  onQueryChange,
  loading,
  onOpen,
  models,
  tools,
  scope,
}: {
  /** Absent while the first page loads: the table draws skeleton rows. */
  page?: AgentsTracesResponse;
  query: DataTableQuery;
  onQueryChange: (
    updater: (previous: DataTableQuery) => DataTableQuery,
  ) => void;
  loading: boolean;
  onOpen: (trace: AgentTraceSummary) => void;
  /** For the query bar's `model:` and `tool:` suggestions. */
  models: string[];
  tools: string[];
  /** The environment and window, beside the query bar. */
  scope: ReactNode;
}) {
  const t = useTranslations('agents');
  const format = useStableFormatter();

  const columns = useMemo(() => {
    const figure = { align: 'end' as const };
    const most = { asc: t('table.most'), desc: t('table.most') };
    const count = (value: number) => (
      <span
        className={
          value === 0 ? 'text-fg-ghost' : 'text-fg-secondary tabular-nums'
        }
      >
        {format.number(value, 'compact')}
      </span>
    );
    return column.columns([
      column.accessor('agent_names', {
        id: 'agents',
        header: t('columns.agents'),
        enableHiding: false,
        enableSorting: false,
        cell: ({ row }) => {
          const trace = row.original;
          return (
            <div className="flex min-w-0 flex-col gap-1 py-1">
              {/* With handoffs, the agents in the order they ran. */}
              <span className="truncate font-medium text-fg text-value">
                {trace.agent_names.length
                  ? trace.agent_names.join(' → ')
                  : t('unnamedAgent')}
              </span>
              <span className="truncate font-mono text-fg-subtle text-mono-sm">
                {trace.trace_id}
              </span>
            </div>
          );
        },
        meta: { label: t('columns.agents') },
      }),
      column.accessor('duration_ms', {
        id: 'duration',
        header: t('columns.duration'),
        cell: ({ getValue }) => {
          const ms = getValue();
          return (
            <span className="text-fg-secondary tabular-nums">
              {ms == null ? '—' : formatSpanDuration(ms)}
            </span>
          );
        },
        meta: {
          label: t('columns.duration'),
          width: 104,
          sortLabels: most,
          ...figure,
        },
      }),
      column.accessor('llm_call_count', {
        id: 'llm_calls',
        header: t('columns.llmCalls'),
        cell: ({ getValue }) => count(getValue()),
        meta: {
          label: t('columns.llmCalls'),
          width: 88,
          sortLabels: most,
          ...figure,
        },
      }),
      column.accessor('tool_call_count', {
        id: 'tool_calls',
        header: t('columns.tools'),
        cell: ({ getValue }) => count(getValue()),
        meta: {
          label: t('columns.tools'),
          width: 120,
          sortLabels: most,
          ...figure,
        },
      }),
      column.accessor('total_tokens', {
        id: 'tokens',
        header: t('columns.tokens'),
        cell: ({ getValue }) => count(getValue()),
        meta: {
          label: t('columns.tokens'),
          width: 96,
          sortLabels: most,
          ...figure,
        },
      }),
      column.accessor('error_count', {
        id: 'errors',
        header: t('columns.errors'),
        cell: ({ getValue }) => {
          const errors = getValue();
          return errors === 0 ? (
            <span className="text-fg-ghost">0</span>
          ) : (
            <Tag variant="soft" tone="error">
              {errors}
            </Tag>
          );
        },
        meta: {
          label: t('columns.errors'),
          width: 88,
          sortLabels: most,
          ...figure,
        },
      }),
      column.accessor('started_at', {
        id: 'newest',
        header: t('columns.started'),
        cell: ({ getValue }) => (
          <span
            className="text-fg-subtle tabular-nums"
            title={format.dateTime(new Date(getValue()), 'dateTime')}
          >
            <RelativeTime at={getValue()} />
          </span>
        ),
        meta: {
          label: t('columns.started'),
          width: 128,
          sortLabels: { asc: t('table.newest'), desc: t('table.newest') },
          ...figure,
        },
      }),
    ]);
  }, [t, format]);

  const fields = useMemo<QueryField[]>(
    () => [
      { key: 'agent', label: t('filters.agent'), variant: 'text' },
      {
        key: 'model',
        label: t('filters.model'),
        variant: 'options',
        multiple: false,
        options: models.map((value) => ({ value, label: value })),
      },
      {
        key: 'tool',
        label: t('filters.tool'),
        variant: 'options',
        multiple: false,
        options: tools.map((value) => ({ value, label: value })),
      },
      {
        key: 'status',
        label: t('filters.status'),
        variant: 'options',
        multiple: false,
        options: [{ value: 'failing', label: t('filters.failing') }],
      },
    ],
    [models, tools, t],
  );

  const table = useDataTable({
    data: page?.items ?? NO_TRACES,
    columns,
    rowCount: page?.total_count ?? 0,
    query,
    onQueryChange,
    getRowId: (row) => row.trace_id,
    storageKey: 'agent-traces',
  });

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      {/* Narrow, the bar takes a row and the scope the next. */}
      <div className="flex flex-wrap items-center gap-2">
        <QueryBar
          className="min-w-0 max-sm:basis-full"
          fields={fields}
          filters={query.filters}
          search={query.search}
          placeholder={t('filters.search')}
          onChange={(next) =>
            onQueryChange((previous) => ({
              ...previous,
              filters: next.filters,
              search: next.search,
              pagination: { ...previous.pagination, pageIndex: 0 },
            }))
          }
        />
        {scope}
      </div>
      <DataTable
        table={table}
        loading={!page || loading}
        className="min-h-0 flex-1"
        onRowClick={(row) => onOpen(row.original)}
        empty={{ title: t('table.empty') }}
      />
    </div>
  );
}
