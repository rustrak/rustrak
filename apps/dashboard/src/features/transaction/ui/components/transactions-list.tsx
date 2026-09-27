import type { OffsetPaginatedResponse, Transaction } from '@rustrak/client';
import { Link, useNavigate } from '@tanstack/react-router';
import { Zap } from 'lucide-react';
import { useTransition } from 'react';
import { useFormatter, useTranslations } from 'use-intl';
import { cn } from '@/shared/lib/utils';
import { Badge } from '@/shared/ui/components/shadcn/badge';
import { TablePagination } from '@/shared/ui/components/table-pagination';

interface TransactionsListProps {
  projectId: number;
  initialTransactions: OffsetPaginatedResponse<Transaction>;
  currentPage: number;
}

function formatDuration(ms: number | null): string {
  if (ms === null) return '—';
  if (ms < 1000) return `${Math.round(ms)}ms`;
  return `${(ms / 1000).toFixed(2)}s`;
}

function durationTone(ms: number | null): {
  text: string;
  bar: string;
  pct: number;
} {
  if (ms === null)
    return { text: 'text-muted-foreground', bar: 'bg-muted', pct: 0 };
  if (ms > 3000)
    return { text: 'text-destructive', bar: 'bg-destructive', pct: 100 };
  if (ms > 1000)
    return {
      text: 'text-yellow-600 dark:text-yellow-500',
      bar: 'bg-yellow-500',
      pct: Math.min(100, (ms / 3000) * 100),
    };
  return {
    text: 'text-primary',
    bar: 'bg-primary',
    pct: Math.min(100, (ms / 3000) * 100),
  };
}

export function TransactionsList({
  projectId,
  initialTransactions,
  currentPage,
}: TransactionsListProps) {
  const format = useFormatter();
  const t = useTranslations('transactions');
  // The samples of one group, on its summary page.
  const navigate = useNavigate({ from: '/projects/$id/performance/summary' });
  const [isPending, startTransition] = useTransition();

  const {
    items: transactions,
    total_count,
    total_pages,
    per_page,
  } = initialTransactions;

  const handlePageChange = (page: number) => {
    // `prev` keeps the group's name and op, so paging stays in context.
    startTransition(() => {
      navigate({ search: (prev) => ({ ...prev, page }) });
    });
  };

  if (transactions.length === 0) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center text-center">
        <Zap className="size-12 text-muted-foreground/30 mb-4" />
        <h2 className="text-lg font-semibold mb-1">{t('empty.title')}</h2>
        <p className="text-sm text-muted-foreground max-w-md">
          {t.rich('empty.sdkHint', {
            code: (chunks) => <code>{chunks}</code>,
          })}
        </p>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 overflow-hidden flex flex-col border rounded-lg">
        <div className="shrink-0 flex items-center gap-4 px-4 py-3 bg-muted/50 border-b">
          <span className="text-xs font-bold uppercase tracking-widest text-muted-foreground flex-1">
            {t('columns.transaction')}
          </span>
          <span className="hidden sm:block text-xs font-bold uppercase tracking-widest text-muted-foreground w-40">
            {t('columns.duration')}
          </span>
          <span className="hidden md:block text-xs font-bold uppercase tracking-widest text-muted-foreground w-28 text-right">
            {t('columns.lastSeen')}
          </span>
        </div>

        <div className="flex-1 overflow-auto divide-y">
          {transactions.map((txn) => {
            const tone = durationTone(txn.duration_ms);
            return (
              <Link
                key={txn.id}
                to="/projects/$id/performance/$txnId"
                params={{ id: projectId, txnId: txn.id }}
                className="flex items-center gap-4 px-4 py-3 border-b last:border-b-0 hover:bg-muted/30 transition-colors group"
              >
                <div className="flex-1 min-w-0">
                  <span className="block font-mono text-sm truncate group-hover:text-primary transition-colors">
                    {txn.transaction_name || t('unnamed')}
                  </span>
                  <div className="flex items-center gap-2 text-xs text-muted-foreground flex-wrap mt-1">
                    {txn.platform && (
                      <Badge variant="outline" className="text-[10px]">
                        {txn.platform}
                      </Badge>
                    )}
                    {txn.environment && (
                      <Badge variant="secondary" className="text-[10px]">
                        {txn.environment}
                      </Badge>
                    )}
                    {txn.release && (
                      <span className="font-mono truncate max-w-40">
                        {txn.release}
                      </span>
                    )}
                    <span className={cn('sm:hidden font-medium', tone.text)}>
                      {formatDuration(txn.duration_ms)}
                    </span>
                  </div>
                </div>

                <div className="hidden sm:flex flex-col items-end w-40 gap-1">
                  <span
                    className={cn('font-mono text-sm font-medium', tone.text)}
                  >
                    {formatDuration(txn.duration_ms)}
                  </span>
                  <div className="h-1.5 w-full rounded-full bg-muted overflow-hidden">
                    <div
                      className={cn('h-full rounded-full', tone.bar)}
                      style={{ width: `${tone.pct}%` }}
                    />
                  </div>
                </div>

                <div className="hidden md:block w-28 text-right">
                  <span className="text-sm text-muted-foreground whitespace-nowrap">
                    {format.relativeTime(new Date(txn.timestamp))}
                  </span>
                </div>
              </Link>
            );
          })}
        </div>
      </div>

      <TablePagination
        currentPage={currentPage}
        totalPages={total_pages}
        totalCount={total_count}
        perPage={per_page}
        disabled={isPending}
        onPageChange={handlePageChange}
      />
    </div>
  );
}
