import type { Monitor, MonitorEnvironmentStatus } from '@rustrak/client';
import { useFormatter, useTranslations } from 'use-intl';
import { cn } from '@/shared/lib/utils';
import { CheckInTimeline } from './check-in-timeline';

const STATUS_DOT: Record<MonitorEnvironmentStatus, string> = {
  ok: 'bg-status-ok',
  error: 'bg-sev-error',
  active: 'bg-muted-foreground/40',
};

/** One card per environment: its health, its clock and its last runs. */
export function MonitorEnvironments({ monitor }: { monitor: Monitor }) {
  const t = useTranslations('crons');
  const format = useFormatter();
  const when = (iso: string | null) =>
    iso ? format.relativeTime(new Date(iso)) : t('never');

  return (
    <div className="space-y-3">
      {monitor.environments.map((env) => (
        <section key={env.id} className="rounded-lg border bg-card p-4">
          <div className="mb-4 flex flex-wrap items-center justify-between gap-x-6 gap-y-2">
            <div className="flex items-center gap-2">
              <span
                aria-hidden="true"
                className={cn('size-2 rounded-full', STATUS_DOT[env.status])}
              />
              <h3 className="text-sm font-semibold">{env.environment}</h3>
              <span className="text-xs text-muted-foreground">
                {t(`environmentStatus.${env.status}`)}
              </span>
            </div>
            <dl className="flex gap-6 text-xs">
              <div>
                <dt className="text-muted-foreground">
                  {t('detail.lastCheckIn')}
                </dt>
                <dd className="font-medium tabular-nums">
                  {when(env.last_checkin)}
                </dd>
              </div>
              <div>
                <dt className="text-muted-foreground">
                  {t('detail.nextCheckIn')}
                </dt>
                <dd className="font-medium tabular-nums">
                  {env.next_checkin
                    ? format.dateTime(new Date(env.next_checkin), 'dateTime')
                    : '—'}
                </dd>
              </div>
            </dl>
          </div>
          <CheckInTimeline
            checkIns={env.recent_checkins}
            slots={48}
            size="lg"
          />
          <Legend />
        </section>
      ))}
    </div>
  );
}

function Legend() {
  const t = useTranslations('crons');
  const items = [
    ['ok', 'bg-status-ok'],
    ['missed', 'bg-sev-warning'],
    ['error', 'bg-sev-error'],
    ['timeout', 'bg-sev-error'],
  ] as const;
  return (
    <div className="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-[11px] text-muted-foreground">
      {items.map(([status, fill]) => (
        <span key={status} className="inline-flex items-center gap-1.5">
          <span
            aria-hidden="true"
            className={cn(
              'size-2 rounded-[2px]',
              fill,
              status === 'timeout' &&
                '[background-image:repeating-linear-gradient(135deg,transparent_0_2px,rgb(255_255_255/0.45)_2px_4px)]',
            )}
          />
          {t(`status.${status}`)}
        </span>
      ))}
    </div>
  );
}
