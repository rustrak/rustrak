import type { Monitor } from '@rustrak/client';
import { useTranslations } from 'use-intl';
import { ScheduleText } from '@/shared/ui/components/schedule-text';

/** What the monitor expects, as the sidebar of its page. */
export function MonitorConfigCard({ monitor }: { monitor: Monitor }) {
  const t = useTranslations('crons');
  const { config } = monitor;
  const rows: [string, React.ReactNode][] = [
    [t('detail.schedule'), <ScheduleText key="s" schedule={config.schedule} />],
    // The words already say all an interval has to say.
    ...(config.schedule.type === 'crontab'
      ? [
          [
            t('detail.rawSchedule'),
            <code key="r" className="font-mono text-xs">
              {config.schedule.value}
            </code>,
          ] as [string, React.ReactNode],
        ]
      : []),
    [t('detail.timezone'), config.timezone],
    [t('detail.margin'), t('detail.minutes', { count: config.checkin_margin })],
    [
      t('detail.maxRuntime'),
      t('detail.minutes', { count: config.max_runtime }),
    ],
    [
      t('detail.failureThreshold'),
      t('detail.failures', { count: config.failure_issue_threshold }),
    ],
    [
      t('detail.recoveryThreshold'),
      t('detail.successes', { count: config.recovery_threshold }),
    ],
    [
      t('detail.slug'),
      <code key="slug" className="font-mono text-xs">
        {monitor.slug}
      </code>,
    ],
  ];

  return (
    <section className="rounded-lg border bg-card">
      <h2 className="border-b px-4 py-3 text-sm font-semibold">
        {t('detail.config')}
      </h2>
      <dl className="divide-y">
        {rows.map(([label, value]) => (
          <div key={label} className="flex flex-col gap-0.5 px-4 py-2.5">
            <dt className="text-xs text-muted-foreground">{label}</dt>
            <dd className="text-sm">{value}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}
