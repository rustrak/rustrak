import type { TransactionStats } from '@rustrak/client';
import { Link } from '@tanstack/react-router';
import { useFormatter, useTranslations } from 'use-intl';

interface TransactionP95BarsProps {
  projectId: number;
  rows: TransactionStats[];
  limit?: number;
}

/** A transaction is called out when this share of its requests fail. */
const FAILING_ABOVE = 0.05;

function formatMs(ms: number): string {
  if (ms >= 1000) {
    return `${(ms / 1000).toFixed(2)}s`;
  }
  return `${Math.round(ms)}ms`;
}

/**
 * Drops the HTTP method from the display name.
 *
 * It is the least distinguishing part ("POST /api/…" for half the list) and in
 * a one-column tile it costs the characters that actually identify the route.
 * The full name stays on the link's title.
 */
function routeLabel(name: string): string {
  return name.replace(/^[A-Z]+\s+/, '') || name;
}

/**
 * The project's slowest transactions by p95, with the failing ones called out.
 *
 * The label sits above its own full-width bar rather than beside it in an axis
 * gutter: in a one-column tile a category axis spends most of the width on
 * truncated names and leaves the bars too short to compare. Stacking gives the
 * name the full width and the bar the full width.
 *
 * Bar length is p95 relative to the slowest in view, so the list reads as "how
 * much worse is the worst" rather than against an absolute scale that would
 * mean nothing without knowing the app.
 */
export function TransactionP95Bars({
  projectId,
  rows,
  limit = 5,
}: TransactionP95BarsProps) {
  const format = useFormatter();
  const t = useTranslations('transactions');
  if (rows.length === 0) {
    return (
      <p className="py-6 text-center text-sm text-muted-foreground">
        {t('p95.empty')}
      </p>
    );
  }

  const visible = [...rows].sort((a, b) => b.p95_ms - a.p95_ms).slice(0, limit);
  const slowest = visible[0]?.p95_ms || 1;

  return (
    <ul className="flex flex-col gap-3.5">
      {visible.map((row) => {
        const failing = row.failure_rate > FAILING_ABOVE;

        return (
          <li key={`${row.transaction_name}-${row.op ?? ''}`}>
            {/* The same drill-down the performance table uses: a row lands on
                the transaction it names, not on the unfiltered list. */}
            <Link
              to="/projects/$id/performance/summary"
              params={{ id: projectId }}
              search={{ name: row.transaction_name, op: row.op || undefined }}
              title={t('p95.barTitle', {
                name: row.transaction_name,
                count: row.count,
              })}
              className="group block"
            >
              <div className="flex items-baseline gap-2">
                <span className="truncate font-mono text-xs font-medium transition-colors group-hover:text-primary">
                  {routeLabel(row.transaction_name)}
                </span>
                <span className="ml-auto shrink-0 text-xs font-semibold tabular-nums">
                  {formatMs(row.p95_ms)}
                </span>
              </div>

              {/* Square at the baseline, 4px rounded data-end: the same mark
                  the error-volume bars use, turned on its side. */}
              <div className="mt-1.5 h-2 w-full overflow-hidden rounded-sm bg-muted">
                <div
                  className="h-full rounded-r-sm"
                  style={{
                    width: `${Math.max((row.p95_ms / slowest) * 100, 2)}%`,
                    // Emphasis: only the transaction in trouble carries colour.
                    background: failing
                      ? 'var(--sev-error)'
                      : 'var(--sev-info)',
                  }}
                />
              </div>

              {/* Labelled selectively: a number on every row would go unread,
                  so only the failing ones say why they are red. */}
              {failing ? (
                <p className="mt-1 text-[11px] tabular-nums text-[color:var(--sev-error)]">
                  {t('p95.failing', {
                    rate: format.number(row.failure_rate, 'percent'),
                  })}
                </p>
              ) : null}
            </Link>
          </li>
        );
      })}
    </ul>
  );
}
