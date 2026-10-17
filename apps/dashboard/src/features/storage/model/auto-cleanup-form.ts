import type { AutoCleanup, CleanupCounts } from '@rustrak/client';
import { z } from 'zod';
import type { Translate } from '@/shared/lib/error-copy';

/**
 * The automatic cleanup form: the server's settings as they are, checked
 * here for what a person can get wrong without asking the server. Whether a
 * crontab parses is still the server's call (`schedules.preview`).
 */
export function autoCleanupFormSchema(t: Translate) {
  return z
    .object({
      enabled: z.boolean(),
      schedule: z.discriminatedUnion('type', [
        z.object({
          type: z.literal('crontab'),
          value: z.string().trim().min(1),
        }),
        z.object({
          type: z.literal('interval'),
          value: z.number().int().min(1),
          unit: z.enum(['minute', 'hour', 'day', 'week', 'month', 'year']),
        }),
      ]),
      timezone: z.string().min(1),
      older_than_days: z.number().int().min(1, t('auto.daysMin')),
      include_events: z.boolean(),
      include_transactions: z.boolean(),
      include_logs: z.boolean(),
    })
    .refine(
      (s) =>
        !s.enabled ||
        s.include_events ||
        s.include_transactions ||
        s.include_logs,
      { message: t('auto.pickOne'), path: ['include_events'] },
    );
}

export type AutoCleanupFormData = z.infer<
  ReturnType<typeof autoCleanupFormSchema>
>;

/** "2 errors", "3 logs"… for what a run removed, leaving out the zeros. */
export function removedParts(removed: CleanupCounts, t: Translate): string[] {
  const parts: [number, string][] = [
    [removed.events, 'unit.errors'],
    [removed.transactions, 'unit.transactions'],
    [removed.spans, 'unit.spans'],
    [removed.logs, 'unit.logs'],
    [removed.issues_removed, 'unit.emptyIssues'],
  ];
  return parts
    .filter(([count]) => count > 0)
    .map(([count, key]) => t(key, { count }));
}

/**
 * A sentence to show: its message key, the instant it is about (formatted
 * relative by the component, which owns the formatter) and its other values.
 */
export interface StatusLine {
  key: string;
  at?: string;
  values?: Record<string, string>;
}

/** When automatic cleanup runs next, and how its last run went. */
export function runStatus(
  state: AutoCleanup,
  t: Translate,
): { next: StatusLine; last: StatusLine | null } {
  const { settings, next_run_at, last_run } = state;
  const next: StatusLine = !settings.enabled
    ? { key: 'auto.off' }
    : next_run_at
      ? { key: 'auto.nextRun', at: next_run_at }
      : { key: 'auto.nextRunPending' };

  if (!last_run) return { next, last: null };
  if (last_run.status === 'running') {
    return {
      next,
      last: { key: 'auto.lastRunRunning', at: last_run.started_at },
    };
  }
  const at = last_run.finished_at ?? last_run.started_at;
  if (last_run.status === 'error') {
    return {
      next,
      last: {
        key: 'auto.lastRunFailed',
        at,
        values: { error: last_run.error ?? '' },
      },
    };
  }
  const parts = last_run.removed ? removedParts(last_run.removed, t) : [];
  return {
    next,
    last: parts.length
      ? { key: 'auto.lastRunRemoved', at, values: { parts: parts.join(', ') } }
      : { key: 'auto.lastRunNothing', at },
  };
}
