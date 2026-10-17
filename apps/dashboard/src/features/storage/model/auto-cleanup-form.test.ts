import type {
  AutoCleanup,
  AutoCleanupSettings,
  CleanupCounts,
} from '@rustrak/client';
import { describe, expect, it } from 'vitest';
import type { Translate } from '@/shared/lib/error-copy';
import {
  autoCleanupFormSchema,
  removedParts,
  runStatus,
} from './auto-cleanup-form';

const t: Translate = (key) => key;
const schema = autoCleanupFormSchema(t);

const settings: AutoCleanupSettings = {
  enabled: true,
  schedule: { type: 'crontab', value: '0 3 * * *' },
  timezone: 'UTC',
  older_than_days: 90,
  include_events: true,
  include_transactions: true,
  include_logs: true,
};

describe('autoCleanupFormSchema', () => {
  it('accepts sensible settings', () => {
    expect(schema.safeParse(settings).success).toBe(true);
  });

  it('needs at least one kind of data when enabled', () => {
    const none = {
      ...settings,
      include_events: false,
      include_transactions: false,
      include_logs: false,
    };
    const parsed = schema.safeParse(none);
    expect(parsed.success).toBe(false);
    expect(parsed.error?.issues[0]?.message).toBe('auto.pickOne');
    expect(schema.safeParse({ ...none, enabled: false }).success).toBe(true);
  });

  it('keeps at least one day of data', () => {
    expect(schema.safeParse({ ...settings, older_than_days: 0 }).success).toBe(
      false,
    );
  });

  it('needs a crontab to be typed', () => {
    expect(
      schema.safeParse({
        ...settings,
        schedule: { type: 'crontab', value: ' ' },
      }).success,
    ).toBe(false);
  });
});

describe('removedParts', () => {
  const counts = (over: Partial<CleanupCounts>): CleanupCounts => ({
    events: 0,
    transactions: 0,
    spans: 0,
    logs: 0,
    issues_removed: 0,
    ...over,
  });

  it('names only what was removed', () => {
    expect(removedParts(counts({ logs: 3, events: 2 }), t)).toEqual([
      'unit.errors',
      'unit.logs',
    ]);
  });

  it('is empty when nothing was removed', () => {
    expect(removedParts(counts({}), t)).toEqual([]);
  });
});

describe('runStatus', () => {
  const state = (over: Partial<AutoCleanup>): AutoCleanup => ({
    settings,
    next_run_at: '2026-01-02T03:00:00Z',
    last_run: null,
    ...over,
  });
  const run = {
    started_at: '2026-01-01T03:00:00Z',
    finished_at: '2026-01-01T03:00:05Z',
    status: 'ok' as const,
    removed: {
      events: 0,
      transactions: 0,
      spans: 0,
      logs: 4,
      issues_removed: 0,
    },
    error: null,
  };

  it('says off when disabled', () => {
    expect(
      runStatus(state({ settings: { ...settings, enabled: false } }), t).next,
    ).toEqual({ key: 'auto.off' });
  });

  it('gives the next run, or that it is being scheduled', () => {
    expect(runStatus(state({}), t).next).toEqual({
      key: 'auto.nextRun',
      at: '2026-01-02T03:00:00Z',
    });
    expect(runStatus(state({ next_run_at: null }), t).next).toEqual({
      key: 'auto.nextRunPending',
    });
  });

  it('reports what the last run removed', () => {
    expect(runStatus(state({ last_run: run }), t).last).toEqual({
      key: 'auto.lastRunRemoved',
      at: '2026-01-01T03:00:05Z',
      values: { parts: 'unit.logs' },
    });
  });

  it('reports a run that removed nothing, failed or is still going', () => {
    const nothing = { ...run, removed: { ...run.removed, logs: 0 } };
    expect(runStatus(state({ last_run: nothing }), t).last?.key).toBe(
      'auto.lastRunNothing',
    );
    const failed = { ...run, status: 'error' as const, error: 'disk full' };
    expect(runStatus(state({ last_run: failed }), t).last).toEqual({
      key: 'auto.lastRunFailed',
      at: '2026-01-01T03:00:05Z',
      values: { error: 'disk full' },
    });
    const running = { ...run, status: 'running' as const, finished_at: null };
    expect(runStatus(state({ last_run: running }), t).last).toEqual({
      key: 'auto.lastRunRunning',
      at: '2026-01-01T03:00:00Z',
    });
  });

  it('has no last run before the first one', () => {
    expect(runStatus(state({}), t).last).toBeNull();
  });
});
