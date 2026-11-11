import type { CheckInResponse, MonitorResponse } from '@rustrak/client';

/**
 * One word for a monitor across its environments, for the list and the
 * header: an open incident anywhere is what a reader must see first.
 */
export type MonitorHealth = 'failing' | 'ok' | 'waiting';

export function monitorHealth(monitor: MonitorResponse): MonitorHealth {
  const statuses = monitor.environments.map((env) => env.status);
  if (statuses.includes('error')) return 'failing';
  if (statuses.includes('ok')) return 'ok';
  return 'waiting';
}

/** The stripe at the start of a row: colour only where it needs a look. */
export const HEALTH_MARK: Record<MonitorHealth, string> = {
  failing: 'bg-sev-error',
  ok: 'bg-success',
  waiting: 'bg-border-control',
};

/**
 * The newest run that finished, across environments: each one's recent
 * check-ins come newest first, but joined they come by environment name.
 */
export function lastRun(monitor: MonitorResponse): CheckInResponse | undefined {
  let newest: CheckInResponse | undefined;
  for (const env of monitor.environments) {
    const run = env.recent_checkins.find((c) => c.status !== 'in_progress');
    if (
      run &&
      (!newest || Date.parse(run.date_added) > Date.parse(newest.date_added))
    ) {
      newest = run;
    }
  }
  return newest;
}

/** The most recent real check-in of any environment. */
export function lastCheckIn(monitor: MonitorResponse): string | null {
  return pick(monitor, 'last_checkin', (a, b) => a > b);
}

/** The soonest expected check-in of any environment. */
export function nextCheckIn(monitor: MonitorResponse): string | null {
  return pick(monitor, 'next_checkin', (a, b) => a < b);
}

/**
 * What "next expected" should say. A miss moves `next_checkin` on to the
 * following slot, so without this a missed run read as "in 2 minutes".
 */
export function nextCheckInState(
  monitor: MonitorResponse,
  now: number,
): 'missed' | 'late' | 'due' | 'none' {
  const missed = monitor.environments.some(
    (env) =>
      env.recent_checkins.find((c) => c.status !== 'in_progress')?.status ===
      'missed',
  );
  if (missed) return 'missed';
  const next = nextCheckIn(monitor);
  if (next === null) return 'none';
  return Date.parse(next) < now ? 'late' : 'due';
}

/** The "next expected" figure's colour: only where something went wrong. */
export const NEXT_TONE: Record<
  ReturnType<typeof nextCheckInState>,
  string | undefined
> = {
  missed: 'text-sev-error-fg',
  late: 'text-sev-warning-fg',
  due: undefined,
  none: undefined,
};

function pick(
  monitor: MonitorResponse,
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

/** What the crons list filters by: a health, or muted. */
export const MONITOR_STATES = ['failing', 'ok', 'waiting', 'muted'] as const;
export type MonitorState = (typeof MONITOR_STATES)[number];

/** Failing first, then waiting, then the rest; by name inside each. */
const RANK: Record<MonitorHealth, number> = { failing: 0, waiting: 1, ok: 2 };

/** The monitors a search and a state show, in the list's order. */
export function filterMonitors(
  monitors: MonitorResponse[],
  q: string,
  state?: MonitorState,
): MonitorResponse[] {
  const text = q.trim().toLowerCase();
  return monitors
    .filter(
      (m) =>
        (!text ||
          m.name.toLowerCase().includes(text) ||
          m.slug.includes(text)) &&
        (!state ||
          (state === 'muted' ? m.is_muted : monitorHealth(m) === state)),
    )
    .sort(
      (a, b) =>
        RANK[monitorHealth(a)] - RANK[monitorHealth(b)] ||
        a.name.localeCompare(b.name),
    );
}
