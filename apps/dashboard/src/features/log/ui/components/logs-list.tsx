import type { Log, OffsetPaginatedResponse } from '@rustrak/client';
import { useNavigate } from '@tanstack/react-router';
import { ScrollText } from 'lucide-react';
import { useMemo } from 'react';
import { useFormatter, useTranslations } from 'use-intl';
import { cn } from '@/shared/lib/utils';
import { expandColumn } from '@/shared/ui/components/data-table/columns';
import { DataTable } from '@/shared/ui/components/data-table/data-table';
import { DataTablePagination } from '@/shared/ui/components/data-table/pagination';
import {
  createAppColumnHelper,
  useAppTable,
} from '@/shared/ui/components/data-table/use-app-table';
import { Badge } from '@/shared/ui/components/shadcn/badge';
import { Button } from '@/shared/ui/components/shadcn/button';
import { Separator } from '@/shared/ui/components/shadcn/separator';
import { useTableUrlState } from '@/shared/ui/hooks/use-table-url-state';

interface LogsListProps {
  initialLogs: OffsetPaginatedResponse<Log>;
  currentPage: number;
  /** Active level filter, if any. */
  activeLevel?: string;
}

/** Severity-ordered levels for the filter bar. */
const LEVELS = ['trace', 'debug', 'info', 'warn', 'error', 'fatal'] as const;

/** Tailwind classes for a level badge, keyed by severity. */
function levelTone(level: string): string {
  switch (level) {
    case 'fatal':
      return 'border-destructive/40 bg-destructive/15 text-destructive';
    case 'error':
      return 'border-destructive/30 bg-destructive/10 text-destructive';
    case 'warn':
      return 'border-yellow-500/30 bg-yellow-500/10 text-yellow-600 dark:text-yellow-500';
    case 'info':
      return 'border-primary/30 bg-primary/10 text-primary';
    case 'debug':
      return 'border-sky-500/30 bg-sky-500/10 text-sky-600 dark:text-sky-400';
    default:
      return 'border-muted-foreground/20 bg-muted text-muted-foreground';
  }
}

/**
 * Renders one attribute value. Logs carry the OTel-style typed map
 * (`{ value, type }`); fall back to a JSON dump for anything else.
 */
function attributeDisplay(raw: unknown): { value: string; type?: string } {
  if (raw !== null && typeof raw === 'object' && 'value' in raw) {
    const entry = raw as { value: unknown; type?: string };
    return { value: String(entry.value), type: entry.type };
  }
  return { value: typeof raw === 'string' ? raw : JSON.stringify(raw) };
}

const helper = createAppColumnHelper<Log>();

/**
 * Columns live at module scope because none of them closes over a prop.
 * `t` is passed in by the component: the columns are built there rather than
 * at module scope because the headers are localized.
 */
/**
 * The formatter is passed in, exactly like `t`.
 *
 * `useFormatter` is a hook, and this is a builder rather than a component, so
 * it cannot reach for one itself. The component that calls it already threads
 * the translator through for the same reason.
 */
type Formatter = ReturnType<typeof useFormatter>;

function buildColumns(
  t: (key: string) => string,
  tableT: (key: string) => string,
  format: Formatter,
) {
  return helper.columns([
    expandColumn<Log>(tableT),
    helper.accessor('level', {
      header: t('columns.level'),
      size: 92,
      minSize: 84,
      cell: ({ getValue }) => (
        <Badge
          variant="outline"
          className={cn(
            'w-full justify-center text-[10px] uppercase tracking-wide',
            levelTone(getValue()),
          )}
        >
          {getValue()}
        </Badge>
      ),
    }),
    helper.accessor('body', {
      id: 'message',
      header: t('columns.message'),
      minSize: 240,
      meta: { grow: true },
      cell: ({ getValue }) => (
        <span className="block truncate font-mono text-sm">
          {getValue() || t('emptyValue')}
        </span>
      ),
    }),
    helper.accessor('trace_id', {
      id: 'trace',
      header: t('columns.trace'),
      size: 150,
      minSize: 110,
      meta: { hideBelow: 'md' },
      cell: ({ getValue }) => {
        const traceId = getValue();
        return traceId ? (
          <span className="font-mono text-xs text-muted-foreground">
            {traceId.slice(0, 8)}…
          </span>
        ) : (
          <span className="text-xs text-muted-foreground/50">—</span>
        );
      },
    }),
    helper.accessor('timestamp', {
      header: t('columns.time'),
      size: 150,
      minSize: 120,
      meta: { align: 'end', hideBelow: 'sm' },
      cell: ({ getValue }) => (
        <span className="whitespace-nowrap text-xs text-muted-foreground">
          {format.relativeTime(new Date(getValue()))}
        </span>
      ),
    }),
  ]);
}

/** The panel under an expanded row: everything the columns had to leave out. */
function LogDetail({ log }: { log: Log }) {
  const format = useFormatter();
  const t = useTranslations('logs');
  const attributes = Object.entries(log.attributes ?? {});

  return (
    <div className="space-y-4 px-5 py-4">
      <dl className="grid grid-cols-[5.5rem_1fr] gap-x-4 gap-y-1.5 text-sm">
        <dt className="text-muted-foreground">timestamp</dt>
        <dd className="font-mono text-xs">
          {format.dateTime(new Date(log.timestamp), 'precise')}
        </dd>
        {log.trace_id && (
          <>
            <dt className="text-muted-foreground">trace_id</dt>
            <dd className="font-mono text-xs break-all">{log.trace_id}</dd>
          </>
        )}
        {log.span_id && (
          <>
            <dt className="text-muted-foreground">span_id</dt>
            <dd className="font-mono text-xs break-all">{log.span_id}</dd>
          </>
        )}
        {log.severity_number !== null && (
          <>
            <dt className="text-muted-foreground">severity</dt>
            <dd className="font-mono text-xs">{log.severity_number}</dd>
          </>
        )}
      </dl>

      {attributes.length > 0 && (
        <div>
          <Separator className="mb-3" />
          <p className="mb-2 text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
            {t('attributes')}
          </p>
          <div className="grid gap-px overflow-hidden rounded-md border bg-border">
            {attributes.map(([key, raw]) => {
              const { value, type } = attributeDisplay(raw);
              return (
                <div
                  key={key}
                  className="grid grid-cols-[12rem_1fr_auto] items-start gap-3 bg-background px-3 py-1.5"
                >
                  <span className="truncate font-mono text-xs text-muted-foreground">
                    {key}
                  </span>
                  <span className="font-mono text-xs break-all">{value}</span>
                  {type && (
                    <Badge variant="secondary" className="text-[10px]">
                      {type}
                    </Badge>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

export function LogsList({
  initialLogs,
  currentPage,
  activeLevel,
}: LogsListProps) {
  const t = useTranslations('logs');
  const tableT = useTranslations('table');
  const format = useFormatter();
  const navigate = useNavigate({ from: '/projects/$id/logs' });
  const { items: logs, total_count, per_page } = initialLogs;

  const urlState = useTableUrlState({
    page: currentPage,
    perPage: per_page,
    navigate: ({ page }) => navigate({ search: (prev) => ({ ...prev, page }) }),
  });

  const filterLevel = (level?: string) =>
    urlState.run(() => navigate({ search: { page: 1, level } }));

  const columns = useMemo(
    () => buildColumns(t, tableT, format),
    [t, tableT, format],
  );

  const table = useAppTable({
    data: logs,
    columns,
    getRowId: (log) => log.id,
    rowCount: total_count,
    state: { pagination: urlState.pagination },
    onPaginationChange: urlState.onPaginationChange,
  });

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 flex-wrap items-center justify-between gap-3 pb-3">
        <div className="flex items-center gap-1 rounded-lg border bg-muted/30 p-1">
          <Button
            variant={!activeLevel ? 'secondary' : 'ghost'}
            size="sm"
            className="h-7 px-3"
            onClick={() => filterLevel()}
            disabled={urlState.isPending}
          >
            {t('allLevels')}
          </Button>
          {LEVELS.map((level) => (
            <Button
              key={level}
              variant={activeLevel === level ? 'secondary' : 'ghost'}
              size="sm"
              className="h-7 px-3 capitalize"
              onClick={() => filterLevel(level)}
              disabled={urlState.isPending}
            >
              {level}
            </Button>
          ))}
        </div>
      </div>

      <DataTable
        table={table}
        density="compact"
        stickyHeader
        isPending={urlState.isPending}
        className="flex-1"
        onRowClick={(row) => row.toggleExpanded()}
        renderDetail={(row) => <LogDetail log={row.original} />}
        empty={
          <div className="flex flex-col items-center justify-center px-6 py-16 text-center">
            <ScrollText className="mb-4 size-12 text-muted-foreground/30" />
            <h2 className="mb-1 text-lg font-semibold">{t('empty.title')}</h2>
            <p className="max-w-md text-sm text-muted-foreground">
              {t('empty.hint')}
            </p>
          </div>
        }
      />

      <DataTablePagination table={table} disabled={urlState.isPending} />
    </div>
  );
}
