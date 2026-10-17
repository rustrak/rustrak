import type { Monitor } from '@rustrak/client';
import { BellOff } from 'lucide-react';
import { useFormatter, useTranslations } from 'use-intl';
import { lastCheckIn, nextCheckIn } from '@/features/monitor/lib/status';
import { Link } from '@/shared/ui/components/link';
import { ScheduleText } from '@/shared/ui/components/schedule-text';
import { CheckInTimeline } from './check-in-timeline';
import { MonitorHealthDot } from './monitor-health';

/**
 * Every monitor of a project, one row each. A monitor with several
 * environments stacks one timeline per environment in its row, so a failing
 * staging job is not hidden behind a healthy production one.
 */
export function MonitorsTable({
  projectId,
  monitors,
}: {
  projectId: number;
  monitors: Monitor[];
}) {
  const t = useTranslations('crons');
  const format = useFormatter();
  const relative = (iso: string | null) =>
    iso ? format.relativeTime(new Date(iso)) : '—';

  return (
    <div className="flex flex-col overflow-hidden rounded-lg border">
      <div className="flex shrink-0 items-center gap-4 border-b bg-muted/50 px-4 py-3 text-xs font-bold uppercase tracking-widest text-muted-foreground whitespace-nowrap">
        <span className="min-w-0 flex-1">{t('columns.monitor')}</span>
        <span className="hidden w-52 lg:block">{t('columns.schedule')}</span>
        <span className="hidden w-[13.5rem] md:block">
          {t('columns.recent')}
        </span>
        <span className="hidden w-28 text-right sm:block">
          {t('columns.lastCheckIn')}
        </span>
        <span className="w-28 text-right">{t('columns.nextCheckIn')}</span>
      </div>
      <div className="divide-y overflow-auto">
        {monitors.map((monitor) => (
          <Link
            key={monitor.id}
            href={`/projects/${projectId}/crons/${encodeURIComponent(monitor.slug)}`}
            className="group flex items-center gap-4 px-4 py-3.5 text-sm transition-colors hover:bg-muted/30"
          >
            <div className="flex min-w-0 flex-1 items-center gap-3">
              <MonitorHealthDot monitor={monitor} />
              <div className="min-w-0">
                <span className="flex items-center gap-1.5 font-medium transition-colors group-hover:text-primary">
                  <span className="truncate">{monitor.name}</span>
                  {monitor.is_muted && (
                    <BellOff
                      className="size-3.5 shrink-0 text-muted-foreground"
                      aria-label={t('health.muted')}
                    />
                  )}
                </span>
                <span className="block truncate font-mono text-xs text-muted-foreground">
                  {monitor.slug}
                </span>
              </div>
            </div>
            <span className="hidden w-52 truncate text-muted-foreground lg:block">
              <ScheduleText
                schedule={monitor.config.schedule}
                timezone={monitor.config.timezone}
              />
            </span>
            <div className="hidden w-[13.5rem] flex-col gap-1.5 md:flex">
              {monitor.environments.length === 0 ? (
                <span className="text-xs text-muted-foreground">
                  {t('health.waiting')}
                </span>
              ) : (
                monitor.environments.map((env) => (
                  <div key={env.id} className="flex flex-col gap-1">
                    {monitor.environments.length > 1 && (
                      <span className="text-[11px] leading-none text-muted-foreground">
                        {env.environment}
                      </span>
                    )}
                    <CheckInTimeline checkIns={env.recent_checkins} />
                  </div>
                ))
              )}
            </div>
            <span className="hidden w-28 text-right text-muted-foreground tabular-nums sm:block">
              {relative(lastCheckIn(monitor))}
            </span>
            <span className="w-28 text-right text-muted-foreground tabular-nums">
              {relative(nextCheckIn(monitor))}
            </span>
          </Link>
        ))}
      </div>
    </div>
  );
}
