import type { MonitorEnvironmentStats, MonitorResponse } from '@rustrak/client';
import { cn, TimeSeriesChart } from '@rustrak/ui';
import type { ReactNode } from 'react';
import { useFormatter, useNow, useTranslations } from 'use-intl';
import { checkInFill } from '@/features/monitor/lib/check-in-fill';
import { formatRunDuration } from '@/features/monitor/lib/duration';
import {
  lastCheckIn,
  lastRun as newestRun,
  nextCheckIn,
} from '@/features/monitor/lib/status';
import { successRate, windowDuration } from '@/features/monitor/lib/timeline';
import { ScheduleText } from '@/shared/ui/components/schedule-text';
import { CheckInStrip, StripAxis } from './check-in-strip';

/**
 * The four figures a monitor is read by: when it last ran and how long it
 * took, when it is due next, and over the window how often it went well and
 * how long it usually takes.
 */
export function MonitorFigures({
  monitor,
  stats,
}: {
  monitor: MonitorResponse;
  stats: MonitorEnvironmentStats[];
}) {
  const t = useTranslations('crons');
  const format = useFormatter();
  const now = useNow().getTime();
  const buckets = stats.flatMap((env) => env.buckets);
  const last = lastCheckIn(monitor);
  const next = nextCheckIn(monitor);
  const late = next !== null && Date.parse(next) < now;
  const lastRun = newestRun(monitor);
  const rate = successRate(buckets);
  const duration = windowDuration(buckets);
  const maxRuntime = monitor.config.max_runtime * 60_000;

  const figures: [string, ReactNode, ReactNode][] = [
    [
      t('detail.lastCheckIn'),
      last ? format.relativeTime(new Date(last)) : '—',
      lastRun?.duration_ms != null
        ? t('detail.took', { duration: formatRunDuration(lastRun.duration_ms) })
        : null,
    ],
    [
      t('detail.nextCheckIn'),
      <span className={late ? 'text-sev-warning-fg' : undefined}>
        {next
          ? late
            ? t('detail.late')
            : format.relativeTime(new Date(next))
          : '—'}
      </span>,
      next ? format.dateTime(new Date(next), 'dateTime') : null,
    ],
    [
      t('detail.successRate'),
      <span
        className={
          rate !== null && rate < 0.95 ? 'text-sev-error-fg' : undefined
        }
      >
        {rate === null
          ? '—'
          : format.number(rate, { style: 'percent', maximumFractionDigits: 1 })}
      </span>,
      t('detail.inWindow'),
    ],
    [
      t('detail.avgDuration'),
      duration === null ? '—' : formatRunDuration(duration),
      t('detail.ofMax', { duration: formatRunDuration(maxRuntime) }),
    ],
  ];

  return (
    <dl className="grid grid-cols-2 gap-px overflow-hidden rounded-2xl border border-border-subtle bg-border-subtle xl:grid-cols-4">
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

/**
 * A strip per environment on one axis, its state beside its name, and what
 * each colour means in this monitor's own terms: how late is missed, how
 * long is a timeout.
 */
export function MonitorTimeline({
  monitor,
  stats,
}: {
  monitor: MonitorResponse;
  stats: MonitorEnvironmentStats[];
}) {
  const t = useTranslations('crons');
  const status = new Map(
    monitor.environments.map((env) => [env.id, env.status]),
  );
  const legend: [Parameters<typeof checkInFill>[0], string][] = [
    ['ok', t('status.ok')],
    ['error', t('status.error')],
    ['timeout', t('legend.timeout', { count: monitor.config.max_runtime })],
    ['missed', t('legend.missed', { count: monitor.config.checkin_margin })],
    ['in_progress', t('status.in_progress')],
  ];

  return (
    <div className="flex flex-col gap-4">
      {stats.map((env) => (
        <div key={env.monitor_environment_id} className="flex flex-col gap-2">
          <div className="flex items-center justify-between gap-3">
            <span className="font-mono text-fg text-mono-sm">
              {env.environment}
            </span>
            <span
              className={cn(
                'text-meta',
                status.get(env.monitor_environment_id) === 'error'
                  ? 'text-sev-error-fg'
                  : 'text-fg-subtle',
              )}
            >
              {t(
                `environmentStatus.${status.get(env.monitor_environment_id) ?? 'active'}`,
              )}
            </span>
          </div>
          <CheckInStrip
            buckets={env.buckets}
            size="lg"
            label={`${monitor.name}, ${env.environment}`}
          />
        </div>
      ))}
      <StripAxis buckets={stats[0]?.buckets ?? []} />
      <ul className="flex flex-wrap gap-x-4 gap-y-1.5 text-fg-subtle text-meta">
        {legend.map(([fill, label]) => (
          <li key={fill} className="flex items-center gap-1.5">
            <span
              aria-hidden="true"
              className={cn('size-2 rounded-xs', checkInFill(fill))}
            />
            {label}
          </li>
        ))}
      </ul>
    </div>
  );
}

/** How long the runs took, bucket by bucket, one line per environment. */
export function DurationChart({ stats }: { stats: MonitorEnvironmentStats[] }) {
  const t = useTranslations('crons');
  const format = useFormatter();
  const [first, second] = stats[0]?.buckets ?? [];
  const daily =
    first && second
      ? Date.parse(second.bucket) - Date.parse(first.bucket) >= 86_400_000
      : false;
  // Series keyed by position, not by name: an environment named `t` would
  // otherwise overwrite the timestamp the axis reads.
  const data = (stats[0]?.buckets ?? []).map((bucket, index) => {
    const point: Record<string, number | null> = {
      t: Date.parse(bucket.bucket),
    };
    stats.forEach((env, at) => {
      point[`env-${at}`] = env.buckets[index]?.avg_duration_ms ?? null;
    });
    return point;
  });

  return (
    <TimeSeriesChart
      data={data}
      series={stats.map((env, index) => ({
        key: `env-${index}`,
        label: env.environment,
        color: `var(--chart-${(index % 5) + 1})`,
      }))}
      xKey="t"
      height={180}
      formatX={(value) =>
        format.dateTime(
          new Date(value),
          daily
            ? { month: 'short', day: 'numeric' }
            : { month: 'short', day: 'numeric', hour: 'numeric' },
        )
      }
      formatY={formatRunDuration}
      label={t('detail.duration')}
    />
  );
}

/** The monitor's configuration, one fact per line, for the rail. */
export function MonitorFacts({ monitor }: { monitor: MonitorResponse }) {
  const t = useTranslations('crons');
  const format = useFormatter();
  const { config } = monitor;
  const facts: [string, ReactNode][] = [
    [
      t('detail.schedule'),
      <ScheduleText schedule={config.schedule} timezone={config.timezone} />,
    ],
    [
      t('detail.rawSchedule'),
      config.schedule.type === 'crontab' ? (
        <span className="font-mono">{config.schedule.value}</span>
      ) : null,
    ],
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
    [t('detail.slug'), <span className="font-mono">{monitor.slug}</span>],
    [
      t('detail.created'),
      <span title={format.dateTime(new Date(monitor.created_at), 'dateTime')}>
        {format.relativeTime(new Date(monitor.created_at))}
      </span>,
    ],
  ];

  return (
    <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2.5 text-meta">
      {facts
        .filter(([, value]) => value)
        .map(([label, value]) => (
          <div key={label} className="contents">
            <dt className="text-fg-subtle">{label}</dt>
            <dd className="min-w-0 text-right text-fg-secondary">{value}</dd>
          </div>
        ))}
    </dl>
  );
}
