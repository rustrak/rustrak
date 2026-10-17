import type { Monitor } from '@rustrak/client';

/**
 * One word for a monitor across its environments, for the list and the
 * header: an open incident anywhere is what a reader must see first.
 */
export type MonitorHealth = 'failing' | 'ok' | 'waiting';

export function monitorHealth(monitor: Monitor): MonitorHealth {
  const statuses = monitor.environments.map((env) => env.status);
  if (statuses.includes('error')) return 'failing';
  if (statuses.includes('ok')) return 'ok';
  return 'waiting';
}

/** The most recent real check-in of any environment. */
export function lastCheckIn(monitor: Monitor): string | null {
  return pick(monitor, 'last_checkin', (a, b) => a > b);
}

/** The soonest expected check-in of any environment. */
export function nextCheckIn(monitor: Monitor): string | null {
  return pick(monitor, 'next_checkin', (a, b) => a < b);
}

function pick(
  monitor: Monitor,
  field: 'last_checkin' | 'next_checkin',
  better: (a: number, b: number) => boolean,
): string | null {
  let best: string | null = null;
  for (const env of monitor.environments) {
    const value = env[field];
    if (
      value &&
      (best === null || better(Date.parse(value), Date.parse(best)))
    ) {
      best = value;
    }
  }
  return best;
}
