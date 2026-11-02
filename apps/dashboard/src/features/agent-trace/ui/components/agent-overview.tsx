import type {
  AgentDurationPoint,
  AgentModelRow,
  AgentSummary,
  AgentTimeseriesPoint,
  AgentToolRow,
} from '@rustrak/client';
import {
  BarsChart,
  cn,
  focusRingInset,
  formatSpanDuration,
  Text,
  TimeSeriesChart,
} from '@rustrak/ui';
import type { ReactNode } from 'react';
import { useFormatter, useTranslations } from 'use-intl';

/**
 * The window in six figures: how many runs, model and tool calls, what they
 * cost in tokens, how long they take, and how many failed -- the error count
 * coloured only when there is one.
 */
export function AgentFigures({ summary }: { summary: AgentSummary }) {
  const t = useTranslations('agents.summary');
  const format = useFormatter();
  const spans = summary.agent_runs + summary.llm_calls + summary.tool_calls;
  const figures: [string, ReactNode, ReactNode][] = [
    [t('agentRuns'), format.number(summary.agent_runs, 'compact'), null],
    [t('llmCalls'), format.number(summary.llm_calls, 'compact'), null],
    [t('toolCalls'), format.number(summary.tool_calls, 'compact'), null],
    [t('totalTokens'), format.number(summary.total_tokens, 'compact'), null],
    [
      t('p95Duration'),
      formatSpanDuration(summary.p95_duration_ms),
      t('avg', { duration: formatSpanDuration(summary.avg_duration_ms) }),
    ],
    [
      t('errors'),
      <span
        className={summary.error_count > 0 ? 'text-sev-error-fg' : undefined}
      >
        {format.number(summary.error_count)}
      </span>,
      spans
        ? t('errorRate', {
            rate: format.number(summary.error_count / spans, {
              style: 'percent',
              maximumFractionDigits: 1,
            }),
          })
        : null,
    ],
  ];

  return (
    <dl className="grid shrink-0 grid-cols-2 gap-px overflow-hidden rounded-2xl border border-border-subtle bg-border-subtle sm:grid-cols-3 xl:grid-cols-6">
      {figures.map(([label, value, footnote]) => (
        <div
          key={label}
          className="flex min-w-0 flex-col gap-1.5 bg-surface px-4 py-3.5"
        >
          <dt className="truncate text-fg-meta text-meta">{label}</dt>
          <dd className="text-fg text-title tabular-nums">{value}</dd>
          <dd className="truncate text-fg-subtle text-hint">
            {footnote ?? ' '}
          </dd>
        </div>
      ))}
    </dl>
  );
}

function useBucketLabel(buckets: { bucket: string }[]) {
  const format = useFormatter();
  const [first, second] = buckets;
  const daily =
    first && second
      ? Date.parse(second.bucket) - Date.parse(first.bucket) >= 86_400_000
      : false;
  return (value: string | number) =>
    format.dateTime(
      new Date(value),
      daily
        ? { month: 'short', day: 'numeric' }
        : { month: 'short', day: 'numeric', hour: 'numeric' },
    );
}

/** Agent runs per bucket, the failed ones stacked on top in red. */
export function RunsChart({ points }: { points: AgentTimeseriesPoint[] }) {
  const t = useTranslations('agents.charts');
  const format = useFormatter();
  const label = useBucketLabel(points);
  return (
    <BarsChart
      data={points.map((p) => ({
        t: Date.parse(p.bucket),
        ok: Math.max(p.value - p.errors, 0),
        failed: p.errors,
      }))}
      series={[
        { key: 'ok', label: t('runsOk'), color: 'var(--chart-1)' },
        { key: 'failed', label: t('runsFailed'), color: 'var(--sev-error)' },
      ]}
      xKey="t"
      height={200}
      formatX={label}
      formatY={(v) => format.number(v, 'compact')}
      label={t('runs')}
    />
  );
}

/** Average and p95 duration of agent runs and model calls per bucket. */
export function DurationChart({ points }: { points: AgentDurationPoint[] }) {
  const t = useTranslations('agents.charts');
  const label = useBucketLabel(points);
  return (
    <TimeSeriesChart
      data={points.map((p) => ({
        t: Date.parse(p.bucket),
        avg: p.avg_ms || null,
        p95: p.p95_ms || null,
      }))}
      series={[
        { key: 'avg', label: t('avg'), color: 'var(--chart-1)' },
        { key: 'p95', label: 'p95', color: 'var(--chart-2)' },
      ]}
      xKey="t"
      height={200}
      formatX={label}
      formatY={axisDuration}
      label={t('duration')}
    />
  );
}

/** A duration short enough for an axis tick: whole seconds, or milliseconds. */
function axisDuration(ms: number): string {
  return ms >= 1000 ? `${Math.round(ms / 1000)}s` : `${Math.round(ms)}ms`;
}

/**
 * A breakdown table: a name, then figures in one weight, the errors red only
 * when there are any. A row opens the traces that used it.
 */
function Breakdown<T>({
  rows,
  empty,
  nameLabel,
  name,
  columns,
  onOpen,
}: {
  rows: T[];
  empty: string;
  nameLabel: string;
  name: (row: T) => string;
  columns: { label: string; value: (row: T) => ReactNode }[];
  onOpen: (row: T) => void;
}) {
  if (rows.length === 0) {
    return (
      <Text variant="meta" tone="subtle" className="px-5 py-4">
        {empty}
      </Text>
    );
  }
  const cell = 'px-5 py-2 whitespace-nowrap';
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-body">
        <thead>
          <tr className="border-border-divider border-y text-column text-fg-meta">
            <th className={cn(cell, 'text-left font-normal')}>{nameLabel}</th>
            {columns.map((c) => (
              <th key={c.label} className={cn(cell, 'text-right font-normal')}>
                {c.label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody className="divide-y divide-border-divider">
          {rows.map((row) => (
            <tr
              key={name(row)}
              tabIndex={0}
              onClick={() => onOpen(row)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') onOpen(row);
              }}
              className={cn(
                'cursor-pointer transition-colors hover:bg-surface-hover',
                focusRingInset,
              )}
            >
              <td
                className={cn(
                  cell,
                  'max-w-64 truncate font-mono text-fg text-mono-sm',
                )}
              >
                {name(row)}
              </td>
              {columns.map((c) => (
                <td
                  key={c.label}
                  className={cn(
                    cell,
                    'text-right text-fg-secondary tabular-nums',
                  )}
                >
                  {c.value(row)}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function Errors({ errors, total }: { errors: number; total: number }) {
  const format = useFormatter();
  if (errors === 0) return <span className="text-fg-ghost">0</span>;
  return (
    <span className="text-sev-error-fg">
      {format.number(errors)}
      <span className="ms-1.5 text-hint">
        {format.number(errors / Math.max(total, 1), {
          style: 'percent',
          maximumFractionDigits: 1,
        })}
      </span>
    </span>
  );
}

export function ModelsTable({
  rows,
  onOpen,
}: {
  rows: AgentModelRow[];
  onOpen: (model: string) => void;
}) {
  const t = useTranslations('agents.modelsTable');
  const format = useFormatter();
  const tokens = (v: number) => format.number(v, 'compact');
  return (
    <Breakdown
      rows={rows}
      empty={t('empty')}
      nameLabel={t('model')}
      name={(row) => row.model}
      onOpen={(row) => onOpen(row.model)}
      columns={[
        { label: t('requests'), value: (r) => format.number(r.requests) },
        {
          label: t('errors'),
          value: (r) => <Errors errors={r.errors} total={r.requests} />,
        },
        { label: t('input'), value: (r) => tokens(r.input_tokens) },
        { label: t('output'), value: (r) => tokens(r.output_tokens) },
        { label: t('avg'), value: (r) => formatSpanDuration(r.avg_ms) },
        { label: t('p95'), value: (r) => formatSpanDuration(r.p95_ms) },
      ]}
    />
  );
}

export function ToolsTable({
  rows,
  onOpen,
}: {
  rows: AgentToolRow[];
  onOpen: (tool: string) => void;
}) {
  const t = useTranslations('agents.toolsTable');
  const format = useFormatter();
  return (
    <Breakdown
      rows={rows}
      empty={t('empty')}
      nameLabel={t('tool')}
      name={(row) => row.tool}
      onOpen={(row) => onOpen(row.tool)}
      columns={[
        { label: t('calls'), value: (r) => format.number(r.calls) },
        {
          label: t('errors'),
          value: (r) => <Errors errors={r.errors} total={r.calls} />,
        },
        { label: t('avg'), value: (r) => formatSpanDuration(r.avg_ms) },
        { label: t('p95'), value: (r) => formatSpanDuration(r.p95_ms) },
      ]}
    />
  );
}
