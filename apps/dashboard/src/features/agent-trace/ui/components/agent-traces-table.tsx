import type { AgentTraceSummary } from '@rustrak/client';
import { Link, useNavigate } from '@tanstack/react-router';
import { useTransition } from 'react';
import { useFormatter, useTranslations } from 'use-intl';
import { cn } from '@/shared/lib/utils';
import { Badge } from '@/shared/ui/components/shadcn/badge';
import { TablePagination } from '@/shared/ui/components/table-pagination';

interface AgentTracesTableProps {
  projectId: number;
  traces: AgentTraceSummary[];
  currentPage: number;
  totalPages: number;
  totalCount: number;
  perPage: number;
}

function formatMs(ms: number | null): string {
  if (ms == null) return '—';
  if (ms < 1000) return `${Math.round(ms)}ms`;
  return `${(ms / 1000).toFixed(2)}s`;
}

/**
 * The AI Agent Monitoring "Traces" table: one row per trace_id, aggregating
 * every AI span in that trace regardless of origin (standalone or
 * transaction-embedded). Same offset-pagination shape and flex-div list
 * pattern as `TransactionStatsTable` for visual consistency.
 */
export function AgentTracesTable({
  projectId,
  traces,
  currentPage,
  totalPages,
  totalCount,
  perPage,
}: AgentTracesTableProps) {
  const format = useFormatter();
  const t = useTranslations('agents');
  const navigate = useNavigate({ from: '/projects/$id/agents/' });
  const [isPending, startTransition] = useTransition();

  const handlePageChange = (page: number) => {
    // `prev` carries the dashboard's filters. Navigating with only `page`
    // would drop them, and a reader paging through a filtered view would
    // silently get unfiltered traces back.
    startTransition(() => {
      navigate({ search: (prev) => ({ ...prev, page }) });
    });
  };

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 overflow-hidden flex flex-col border rounded-lg">
        <div className="shrink-0 flex items-center gap-4 px-4 py-3 bg-muted/50 border-b text-xs font-bold uppercase tracking-widest text-muted-foreground">
          <span className="flex-1">{t('columns.agents')}</span>
          <span className="w-24 text-right">{t('columns.started')}</span>
          <span className="w-24 text-right">{t('columns.duration')}</span>
          <span className="w-16 text-right">{t('columns.llmCalls')}</span>
          <span className="w-16 text-right">{t('columns.tools')}</span>
          <span className="w-20 text-right">{t('columns.tokens')}</span>
          <span className="w-16 text-right">{t('columns.errors')}</span>
        </div>
        <div className="flex-1 overflow-auto divide-y">
          {traces.map((trace) => (
            <Link
              key={trace.trace_id}
              to="/projects/$id/agents/$traceId"
              params={{ id: projectId, traceId: trace.trace_id }}
              className="flex items-center gap-4 px-4 py-3 text-sm hover:bg-muted/30 transition-colors group"
            >
              <div className="flex-1 min-w-0">
                {trace.agent_names.length > 0 ? (
                  <span className="flex flex-wrap items-center gap-1">
                    {trace.agent_names.map((name) => (
                      <Badge
                        key={name}
                        variant="secondary"
                        className="font-mono font-normal"
                      >
                        {name}
                      </Badge>
                    ))}
                  </span>
                ) : (
                  <span className="block font-mono truncate text-muted-foreground">
                    {t('unnamedAgent')}
                  </span>
                )}
                <span className="mt-0.5 block truncate text-xs text-muted-foreground">
                  {trace.trace_id}
                </span>
              </div>
              <span className="w-24 text-right font-mono text-xs tabular-nums text-muted-foreground">
                {format.relativeTime(new Date(trace.started_at))}
              </span>
              <span className="w-24 text-right font-mono tabular-nums text-muted-foreground">
                {formatMs(trace.duration_ms)}
              </span>
              <span className="w-16 text-right font-mono tabular-nums text-muted-foreground">
                {format.number(trace.llm_call_count)}
              </span>
              <span className="w-16 text-right font-mono tabular-nums text-muted-foreground">
                {format.number(trace.tool_call_count)}
              </span>
              <span className="w-20 text-right font-mono tabular-nums text-muted-foreground">
                {format.number(trace.total_tokens, 'compact')}
              </span>
              {/* The only column that colours itself, and only when non-zero:
                  a table where every cell competes for attention has none. */}
              <span
                className={cn(
                  'w-16 text-right font-mono tabular-nums',
                  trace.error_count > 0
                    ? 'font-semibold text-destructive'
                    : 'text-muted-foreground',
                )}
              >
                {format.number(trace.error_count)}
              </span>
            </Link>
          ))}
        </div>
      </div>

      <TablePagination
        currentPage={currentPage}
        totalPages={totalPages}
        totalCount={totalCount}
        perPage={perPage}
        disabled={isPending}
        onPageChange={handlePageChange}
      />
    </div>
  );
}
