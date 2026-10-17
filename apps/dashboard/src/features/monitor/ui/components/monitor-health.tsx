import type { Monitor } from '@rustrak/client';
import { BellOff } from 'lucide-react';
import { useTranslations } from 'use-intl';
import {
  type MonitorHealth,
  monitorHealth,
} from '@/features/monitor/lib/status';
import { cn } from '@/shared/lib/utils';

const DOT: Record<MonitorHealth, string> = {
  failing: 'bg-sev-error',
  ok: 'bg-status-ok',
  waiting: 'bg-muted-foreground/40',
};

const PILL: Record<MonitorHealth, string> = {
  failing: 'border-sev-error/30 bg-sev-error/10 text-sev-error',
  ok: 'border-status-ok/30 bg-status-ok/10 text-status-ok',
  waiting: 'border-border bg-muted text-muted-foreground',
};

/** A dot in the colour of the monitor's health, for dense rows. */
export function MonitorHealthDot({ monitor }: { monitor: Monitor }) {
  const t = useTranslations('crons');
  const health = monitorHealth(monitor);
  return (
    <span
      role="img"
      aria-label={t(`health.${health}`)}
      className={cn('size-2.5 shrink-0 rounded-full', DOT[health])}
    />
  );
}

/** The monitor's health spelled out, plus whether it is muted. */
export function MonitorHealthBadge({ monitor }: { monitor: Monitor }) {
  const t = useTranslations('crons');
  const health = monitorHealth(monitor);
  return (
    <span className="inline-flex items-center gap-2">
      <span
        className={cn(
          'inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-xs font-semibold',
          PILL[health],
        )}
      >
        <span
          aria-hidden="true"
          className={cn('size-1.5 rounded-full', DOT[health])}
        />
        {t(`health.${health}`)}
      </span>
      {monitor.is_muted && (
        <span className="inline-flex items-center gap-1 rounded-full border px-2.5 py-0.5 text-xs font-medium text-muted-foreground">
          <BellOff className="size-3" aria-hidden="true" />
          {t('health.muted')}
        </span>
      )}
    </span>
  );
}
