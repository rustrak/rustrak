import type { TransactionStats } from '@rustrak/client';
import { Link, useNavigate } from '@tanstack/react-router';
import { useTransition } from 'react';
import { useFormatter, useTranslations } from 'use-intl';
import { cn } from '@/shared/lib/utils';
import { Badge } from '@/shared/ui/components/shadcn/badge';
import { TablePagination } from '@/shared/ui/components/table-pagination';

interface TransactionStatsTableProps {
  projectId: number;
  stats: TransactionStats[];
  currentPage: number;
  totalPages: number;
  totalCount: number;
  perPage: number;
}

function formatMs(ms: number): string {
  if (ms < 1) return `${(ms * 1000).toFixed(0)}µs`;
  if (ms < 1000) return `${Math.round(ms)}ms`;
  return `${(ms / 1000).toFixed(2)}s`;
}

// Latency tone mirrors the list view: green < 1s, amber < 3s, red beyond.
function latencyTone(ms: number): string {
  if (ms > 3000) return 'text-destructive';
  if (ms > 1000) return 'text-yellow-600 dark:text-yellow-500';
  return 'text-foreground';
}

function failureTone(rate: number): string {
  if (rate >= 0.05) return 'text-destructive';
  if (rate > 0) return 'text-yellow-600 dark:text-yellow-500';
  return 'text-muted-foreground';
}

/**
 * The performance overview: one row per (transaction, op) group with throughput
 * and latency percentiles. Offset-paginated like every other table. Rows link
 * into the group's samples.
 */
export function TransactionStatsTable({
  projectId,
  stats,
  currentPage,
  totalPages,
  totalCount,
  perPage,
}: TransactionStatsTableProps) {
  const format = useFormatter();
  const t = useTranslations('transactions');
  const navigate = useNavigate({ from: '/projects/$id/performance/' });
  const [isPending, startTransition] = useTransition();

  const handlePageChange = (page: number) => {
    startTransition(() => {
      navigate({ search: (prev) => ({ ...prev, page }) });
    });
  };

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 overflow-hidden flex flex-col border rounded-lg">
        <div className="shrink-0 flex items-center gap-4 px-4 py-3 bg-muted/50 border-b text-xs font-bold uppercase tracking-widest text-muted-foreground">
          <span className="flex-1">{t('columns.transaction')}</span>
          <span className="hidden sm:block w-20 text-right">
            {t('columns.count')}
          </span>
          <span className="w-16 text-right">p50</span>
          <span className="w-16 text-right">p95</span>
          <span className="hidden md:block w-16 text-right">p99</span>
          <span className="w-20 text-right">{t('columns.failures')}</span>
        </div>
        <div className="flex-1 overflow-auto divide-y">
          {stats.map((s) => {
            const key = `${s.transaction_name}⋄${s.op ?? ''}`;
            return (
              // Rows open the group's samples, Sentry's transaction summary.
              <Link
                key={key}
                to="/projects/$id/performance/summary"
                params={{ id: projectId }}
                search={{ name: s.transaction_name, op: s.op || undefined }}
                className="flex items-center gap-4 px-4 py-3 text-sm hover:bg-muted/30 transition-colors group"
              >
                <div className="flex-1 min-w-0">
                  <span className="block font-mono truncate group-hover:text-primary transition-colors">
                    {s.transaction_name || t('unnamed')}
                  </span>
                  {s.op && (
                    <Badge variant="secondary" className="text-[10px] mt-1">
                      {s.op}
                    </Badge>
                  )}
                </div>
                <span className="hidden sm:block w-20 text-right font-mono tabular-nums text-muted-foreground">
                  {format.number(s.count)}
                </span>
                <span
                  className={cn(
                    'w-16 text-right font-mono tabular-nums',
                    latencyTone(s.p50_ms),
                  )}
                >
                  {formatMs(s.p50_ms)}
                </span>
                <span
                  className={cn(
                    'w-16 text-right font-mono tabular-nums',
                    latencyTone(s.p95_ms),
                  )}
                >
                  {formatMs(s.p95_ms)}
                </span>
                <span
                  className={cn(
                    'hidden md:block w-16 text-right font-mono tabular-nums',
                    latencyTone(s.p99_ms),
                  )}
                >
                  {formatMs(s.p99_ms)}
                </span>
                <span
                  className={cn(
                    'w-20 text-right font-mono tabular-nums',
                    failureTone(s.failure_rate),
                  )}
                >
                  {(s.failure_rate * 100).toFixed(1)}%
                </span>
              </Link>
            );
          })}
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
