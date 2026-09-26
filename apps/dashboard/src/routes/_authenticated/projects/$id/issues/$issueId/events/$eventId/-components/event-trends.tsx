import type { IssueAggregates, IssueStats } from '@rustrak/client';
import { useFormatter, useTranslations } from 'use-intl';
import { TagDistribution } from '@/features/issue/ui/components/tag-distribution';
import { EventChart } from '@/shared/ui/components/event-chart';

/**
 * The issue's last thirty days above the event: its counts and chart, and the
 * tags it spreads across.
 *
 * Both inputs are decoration. `null` means the request failed, and renders the
 * same empty state as an issue that genuinely has no data.
 */
export function EventTrends({
  stats,
  aggregates,
}: {
  stats: IssueStats | null;
  aggregates: IssueAggregates | null;
}) {
  const t = useTranslations('projectPages');
  const format = useFormatter();
  const buckets = stats?.data ?? [];
  const total = buckets.reduce((sum, [, count]) => sum + count, 0);
  const tags = aggregates?.tags ?? [];

  return (
    <div className="grid grid-cols-1 lg:grid-cols-[1fr_340px] gap-4">
      <div className="rounded-lg border bg-card p-4 flex gap-5">
        <div className="shrink-0 space-y-3">
          <Counter
            label={t('event.events')}
            value={format.number(total, 'compact')}
          />
          <Counter
            label={t('event.users')}
            value={format.number(aggregates?.user_count ?? 0, 'compact')}
          />
        </div>
        <div className="flex-1 min-w-0">
          {buckets.length > 0 ? (
            <EventChart data={buckets} />
          ) : (
            <div className="h-[130px] flex items-center justify-center text-xs text-muted-foreground">
              {t('event.noEventData')}
            </div>
          )}
        </div>
      </div>

      <div className="rounded-lg border bg-card p-4">
        <h3 className="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground mb-3">
          {t('event.tags')}
        </h3>
        {tags.length > 0 ? (
          <TagDistribution tags={tags.slice(0, 5)} />
        ) : (
          <p className="text-xs text-muted-foreground">{t('event.noTags')}</p>
        )}
      </div>
    </div>
  );
}

function Counter({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <p className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
        {label}
      </p>
      <p className="text-xl font-semibold tabular-nums">{value}</p>
    </div>
  );
}
