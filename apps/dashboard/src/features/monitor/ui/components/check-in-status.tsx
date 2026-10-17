import type { CheckInStatus } from '@rustrak/client';
import { useTranslations } from 'use-intl';
import { cn } from '@/shared/lib/utils';

/**
 * A diagonal hatch over a fill, for the two statuses the server inferred
 * rather than the job reported: a reader must tell a timeout from a failure
 * without telling red from red (Sentry's `getTickStyle`).
 */
const HATCH =
  '[background-image:repeating-linear-gradient(135deg,transparent_0_2px,rgb(255_255_255/0.45)_2px_4px)]';

const FILL: Record<CheckInStatus, string> = {
  ok: 'bg-status-ok',
  error: 'bg-sev-error',
  timeout: cn('bg-sev-error', HATCH),
  missed: 'bg-sev-warning',
  in_progress: 'bg-muted-foreground/40 animate-pulse',
  unknown: cn('bg-muted-foreground/40', HATCH),
};

/** The fill of one check-in, as a tick on a timeline or a status dot. */
export function checkInFill(status: CheckInStatus): string {
  return FILL[status];
}

/** A status dot and its name, never the colour alone. */
export function CheckInStatusLabel({ status }: { status: CheckInStatus }) {
  const t = useTranslations('crons');
  return (
    <span className="inline-flex items-center gap-2">
      <span
        aria-hidden="true"
        className={cn('size-2.5 shrink-0 rounded-full', checkInFill(status))}
      />
      {t(`status.${status}`)}
    </span>
  );
}
