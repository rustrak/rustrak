import type { IssueStatsResponse } from '@rustrak/client';
import { BarsChart, Text } from '@rustrak/ui';
import { useFormatter, useTranslations } from 'use-intl';

/**
 * The issue over the last thirty days, as the first thing under the header:
 * whether it is getting worse is the question every reader brings.
 *
 * Lifetime totals are the rail's; this panel counts the window it draws, so
 * the figure and the bars always agree. No user count here: the one there is
 * read from the latest events with no date bound, and beside "30 days" it
 * would claim a window it was not counted over. The rail has it.
 */
export function EventTrends({ stats }: { stats: IssueStatsResponse | null }) {
  const t = useTranslations('issues');
  const format = useFormatter();
  // The spec types each bucket as `number[]`; the server sends a
  // `[timestamp, count]` pair, the timestamp in seconds.
  const buckets = (stats?.data ?? []) as [number, number][];
  const total = buckets.reduce((sum, [, count]) => sum + count, 0);
  const data = buckets.map(([ts, count]) => ({
    t: ts < 1e12 ? ts * 1000 : ts,
    events: count,
  }));

  return (
    <section className="rounded-xl border border-border-subtle bg-surface">
      <header className="flex flex-wrap items-end justify-between gap-4 px-5 pt-4">
        <div>
          <h2 className="text-card-title text-fg">{t('detail.activity30d')}</h2>
          <Text variant="meta" tone="subtle">
            {t('detail.activity30dHint')}
          </Text>
        </div>
        <dl className="flex gap-6">
          <Figure label={t('columns.events')} value={format.number(total)} />
        </dl>
      </header>
      <div className="px-3 pt-3 pb-4">
        {data.length > 0 ? (
          <BarsChart
            data={data}
            series={[
              {
                key: 'events',
                label: t('columns.events'),
                color: 'var(--chart-1)',
              },
            ]}
            xKey="t"
            integer
            height={140}
            formatX={(value) =>
              format.dateTime(new Date(value), {
                month: 'short',
                day: 'numeric',
              })
            }
            formatY={(value) => format.number(value, 'compact')}
            label={t('detail.activity30d')}
          />
        ) : (
          <p className="flex h-36 items-center justify-center text-meta text-fg-subtle">
            {t('detail.noActivity')}
          </p>
        )}
      </div>
    </section>
  );
}

function Figure({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col sm:items-end">
      <dt className="text-hint text-fg-meta">{label}</dt>
      <dd className="text-section text-fg tabular-nums">{value}</dd>
    </div>
  );
}
