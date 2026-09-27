import type {
  OffsetPaginatedResponse,
  ReleaseHealthRow,
} from '@rustrak/client';
import { Link, useNavigate } from '@tanstack/react-router';
import { Rocket } from 'lucide-react';
import { useTransition } from 'react';
import { useFormatter, useTranslations } from 'use-intl';
import {
  crashFreeClass,
  pct,
  RELEASE_PERIODS,
  type ReleasePeriod,
} from '@/features/release/model/session-health';
import { cn } from '@/shared/lib/utils';
import { Badge } from '@/shared/ui/components/shadcn/badge';
import { Button } from '@/shared/ui/components/shadcn/button';
import { TablePagination } from '@/shared/ui/components/table-pagination';

interface ReleasesListProps {
  projectId: number;
  initialHealth: OffsetPaginatedResponse<ReleaseHealthRow>;
  currentPage: number;
  /** Active period filter, if any (omitted = all time). */
  activePeriod?: ReleasePeriod;
}

/**
 * The releases overview: one row per (release, environment) with session
 * volume and crash-free rates. Offset-paginated like every other table, with
 * both the page and the period filter held in the URL so the view is
 * shareable and survives a refresh. Rows link into the release detail.
 */
export function ReleasesList({
  projectId,
  initialHealth,
  currentPage,
  activePeriod,
}: ReleasesListProps) {
  const format = useFormatter();
  const t = useTranslations('releases');
  const navigate = useNavigate({ from: '/projects/$id/releases/' });
  const [isPending, startTransition] = useTransition();

  const { items: rows, total_count, total_pages, per_page } = initialHealth;

  const go = (page: number, period?: ReleasePeriod) => {
    startTransition(() => {
      navigate({ search: { page, period } });
    });
  };

  return (
    <div className="flex flex-col h-full">
      {/* Filter bar — changing the period resets to the first page. */}
      <div className="shrink-0 flex items-center gap-3 pb-3 flex-wrap">
        <div className="flex items-center gap-1 rounded-lg border bg-muted/30 p-1">
          <Button
            variant={!activePeriod ? 'secondary' : 'ghost'}
            size="sm"
            className="h-7 px-3"
            onClick={() => go(1)}
            disabled={isPending}
          >
            {t('all')}
          </Button>
          {RELEASE_PERIODS.map((period) => (
            <Button
              key={period}
              variant={activePeriod === period ? 'secondary' : 'ghost'}
              size="sm"
              className="h-7 px-3"
              onClick={() => go(1, period)}
              disabled={isPending}
            >
              {period}
            </Button>
          ))}
        </div>
      </div>

      {rows.length === 0 ? (
        <div className="flex-1 flex flex-col items-center justify-center text-center rounded-lg border border-dashed">
          <Rocket className="size-12 text-muted-foreground/30 mb-4" />
          <h2 className="text-lg font-semibold mb-1">{t('empty.title')}</h2>
          <p className="text-sm text-muted-foreground max-w-md">
            {t('empty.hint')}
          </p>
        </div>
      ) : (
        <div className="flex-1 overflow-hidden flex flex-col border rounded-lg">
          {/* whitespace-nowrap: the crash-free labels are long enough to wrap
              onto a second line and desync the header from the rows. */}
          <div className="shrink-0 flex items-center gap-4 px-4 py-3 bg-muted/50 border-b text-xs font-bold uppercase tracking-widest text-muted-foreground whitespace-nowrap">
            <span className="flex-1">{t('columns.release')}</span>
            <span className="w-20 text-right">{t('sessions')}</span>
            <span className="hidden sm:block w-40 text-right">
              {t('crashFree')}
            </span>
            <span className="hidden md:block w-40 text-right">
              {t('crashFreeUsers')}
            </span>
            <span className="w-20 text-right">{t('crashed')}</span>
          </div>
          <div className="flex-1 overflow-auto divide-y">
            {rows.map((row) => (
              <Link
                key={`${row.release}-${row.environment}`}
                to="/projects/$id/releases/$release"
                params={{ id: projectId, release: row.release }}
                search={{ environment: row.environment }}
                className="flex items-center gap-4 px-4 py-3 text-sm hover:bg-muted/30 transition-colors group"
              >
                <div className="flex-1 min-w-0">
                  <span className="block font-mono truncate group-hover:text-primary transition-colors">
                    {row.release}
                  </span>
                  <Badge variant="secondary" className="text-[10px] mt-1">
                    {row.environment}
                  </Badge>
                </div>
                <span className="w-20 text-right font-mono tabular-nums text-muted-foreground">
                  {format.number(row.total)}
                </span>
                <span
                  className={cn(
                    'hidden sm:block w-40 text-right font-mono tabular-nums',
                    crashFreeClass(row.crash_free_sessions_rate),
                  )}
                >
                  {pct(row.crash_free_sessions_rate)}
                </span>
                <span
                  className={cn(
                    'hidden md:block w-40 text-right font-mono tabular-nums',
                    crashFreeClass(row.crash_free_users_rate),
                  )}
                >
                  {pct(row.crash_free_users_rate)}
                </span>
                <span
                  className={cn(
                    'w-20 text-right font-mono tabular-nums',
                    row.crashed > 0
                      ? 'text-red-600 dark:text-red-400'
                      : 'text-muted-foreground',
                  )}
                >
                  {row.crashed > 0 ? format.number(row.crashed) : '—'}
                </span>
              </Link>
            ))}
          </div>
        </div>
      )}

      <TablePagination
        currentPage={currentPage}
        totalPages={total_pages}
        totalCount={total_count}
        perPage={per_page}
        disabled={isPending}
        onPageChange={(page) => go(page, activePeriod)}
      />
    </div>
  );
}
