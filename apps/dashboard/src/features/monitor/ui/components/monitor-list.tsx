import type { MonitorEnvironmentStats, MonitorResponse } from '@rustrak/client';
import { cn, focusRingInset, MuteIcon, Skeleton, Text } from '@rustrak/ui';
import { useFormatter, useNow, useTranslations } from 'use-intl';
import { formatRunDuration } from '@/features/monitor/lib/duration';
import { monitorHref } from '@/features/monitor/lib/href';
import {
  HEALTH_MARK,
  lastCheckIn,
  monitorHealth,
  lastRun as newestRun,
  nextCheckIn,
} from '@/features/monitor/lib/status';
import { Link } from '@/shared/ui/components/link';
import { ScheduleText } from '@/shared/ui/components/schedule-text';
import { CheckInStrip, StripAxis } from './check-in-strip';

const COLUMNS =
  'md:grid-cols-[minmax(0,1.2fr)_minmax(0,2fr)_8rem_8rem] xl:grid-cols-[minmax(0,1.2fr)_minmax(0,0.9fr)_minmax(0,2fr)_8rem_8rem]';

/**
 * The monitors, the failing ones first: which job, when it runs, how its
 * runs went over the window -- a strip per environment, every strip on the
 * same axis -- and when it last and next checks in. A muted one is dimmed.
 */
export function MonitorList({
  projectId,
  monitors,
  stats,
  loading,
}: {
  projectId: number;
  /** Absent while the monitors load: the list draws skeleton rows. */
  monitors?: MonitorResponse[];
  stats?: MonitorEnvironmentStats[];
  /** The rows on screen are for an address the list is leaving. */
  loading?: boolean;
}) {
  const t = useTranslations('crons');
  const axis = stats?.[0]?.buckets ?? [];

  return (
    <div
      aria-busy={loading}
      className={cn(
        'flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-border bg-panel',
        loading && monitors && 'opacity-60',
      )}
    >
      <div
        className={cn(
          'hidden shrink-0 items-end gap-4 border-border-divider border-b px-4 py-2.5 font-mono text-column text-fg-meta uppercase md:grid',
          COLUMNS,
        )}
      >
        <span>{t('columns.monitor')}</span>
        <span className="hidden xl:block">{t('columns.schedule')}</span>
        <StripAxis buckets={axis} className="normal-case" />
        <span className="truncate text-right">{t('columns.lastCheckIn')}</span>
        <span className="truncate text-right">{t('columns.nextCheckIn')}</span>
      </div>
      <ul className="min-h-0 flex-1 divide-y divide-border-divider overflow-y-auto">
        {!monitors ? (
          <MonitorRowsSkeleton />
        ) : monitors.length === 0 ? (
          <li className="px-4 py-16 text-center">
            <Text variant="meta" tone="subtle">
              {t('noMatch')}
            </Text>
          </li>
        ) : (
          monitors.map((monitor) => (
            <MonitorRow
              key={monitor.id}
              projectId={projectId}
              monitor={monitor}
              stats={(stats ?? []).filter(
                (s) => s.monitor_slug === monitor.slug,
              )}
            />
          ))
        )}
      </ul>
    </div>
  );
}

/** Rows in the list's columns, at the size of what replaces them. */
function MonitorRowsSkeleton() {
  return Array.from({ length: 5 }, (_, row) => (
    <li
      // Placeholder rows have no identity beyond their position.
      // react-doctor-disable-next-line react-doctor/no-array-index-as-key
      key={row}
      className={cn(
        'grid grid-cols-1 gap-x-4 gap-y-2.5 px-4 py-3 md:items-center',
        COLUMNS,
      )}
    >
      <span className="flex items-center gap-3">
        <Skeleton className="h-8 w-0.75" />
        <span className="flex flex-col gap-1.5">
          <Skeleton className="h-4 w-36" />
          <Skeleton className="h-3 w-24" />
        </span>
      </span>
      <Skeleton className="hidden h-3 w-28 xl:block" />
      <Skeleton className="h-5 w-full" />
      <Skeleton className="hidden h-3 w-16 justify-self-end md:block" />
      <Skeleton className="hidden h-3 w-16 justify-self-end md:block" />
    </li>
  ));
}

function MonitorRow({
  projectId,
  monitor,
  stats,
}: {
  projectId: number;
  monitor: MonitorResponse;
  stats: MonitorEnvironmentStats[];
}) {
  const t = useTranslations('crons');
  const format = useFormatter();
  const now = useNow().getTime();
  const last = lastCheckIn(monitor);
  const next = nextCheckIn(monitor);
  const lastRun = newestRun(monitor);
  const late = next !== null && Date.parse(next) < now;

  return (
    <li>
      <Link
        href={monitorHref(projectId, monitor.slug)}
        className={cn(
          'grid grid-cols-1 gap-x-4 gap-y-2.5 px-4 py-3 transition-colors hover:bg-surface-hover md:items-center',
          COLUMNS,
          focusRingInset,
          monitor.is_muted && 'opacity-60',
        )}
      >
        <span className="flex min-w-0 items-center gap-3">
          <span
            aria-hidden="true"
            className={cn(
              'h-8 w-0.75 shrink-0 rounded-pill',
              HEALTH_MARK[monitorHealth(monitor)],
            )}
          />
          <span className="flex min-w-0 flex-1 flex-col gap-0.5">
            <span className="flex min-w-0 items-center gap-1.5">
              <span className="truncate font-medium text-body text-fg">
                {monitor.name}
              </span>
              {monitor.is_muted ? (
                <MuteIcon
                  size="sm"
                  className="shrink-0 text-fg-subtle"
                  aria-label={t('health.muted')}
                />
              ) : null}
            </span>
            <span className="truncate text-fg-subtle text-meta">
              <span className="font-mono text-mono-sm">{monitor.slug}</span>
              <span className="xl:hidden">
                {' · '}
                <ScheduleText
                  schedule={monitor.config.schedule}
                  timezone={monitor.config.timezone}
                />
              </span>
            </span>
          </span>
          {/* Narrow, the last check-in rides on the name's line. */}
          <span className="shrink-0 text-fg-subtle text-meta tabular-nums md:hidden">
            {last ? format.relativeTime(new Date(last)) : t('never')}
          </span>
        </span>

        <span className="hidden min-w-0 truncate text-fg-secondary text-meta xl:block">
          <ScheduleText
            schedule={monitor.config.schedule}
            timezone={monitor.config.timezone}
          />
        </span>

        <span className="flex min-w-0 flex-col gap-1.5">
          {stats.length === 0 ? (
            <span className="text-fg-subtle text-meta">
              {t('health.waiting')}
            </span>
          ) : (
            stats.map((env) => (
              <span
                key={env.monitor_environment_id}
                className="flex min-w-0 flex-col gap-1"
              >
                {stats.length > 1 ? (
                  <span className="font-mono text-fg-subtle text-mono-sm leading-none">
                    {env.environment}
                  </span>
                ) : null}
                <CheckInStrip
                  buckets={env.buckets}
                  label={`${monitor.name}, ${env.environment}`}
                />
              </span>
            ))
          )}
        </span>

        <span className="hidden flex-col items-end gap-0.5 text-meta tabular-nums md:flex">
          <span className="truncate text-fg-secondary">
            {last ? format.relativeTime(new Date(last)) : t('never')}
          </span>
          {lastRun?.duration_ms != null ? (
            <span className="text-fg-subtle">
              {formatRunDuration(lastRun.duration_ms)}
            </span>
          ) : null}
        </span>
        <span
          className={cn(
            'hidden truncate text-right text-meta tabular-nums md:block',
            late ? 'text-sev-warning-fg' : 'text-fg-secondary',
          )}
        >
          {next
            ? late
              ? t('late')
              : format.relativeTime(new Date(next))
            : '—'}
        </span>
      </Link>
    </li>
  );
}
