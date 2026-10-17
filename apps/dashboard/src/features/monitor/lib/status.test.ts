import type { Monitor, MonitorEnvironment } from '@rustrak/client';
import { describe, expect, it } from 'vitest';
import { lastCheckIn, monitorHealth, nextCheckIn } from './status';

function env(overrides: Partial<MonitorEnvironment>): MonitorEnvironment {
  return {
    id: crypto.randomUUID(),
    environment: 'production',
    status: 'ok',
    next_checkin: null,
    next_checkin_latest: null,
    last_checkin: null,
    recent_checkins: [],
    ...overrides,
  };
}

function monitor(environments: MonitorEnvironment[]): Monitor {
  return {
    id: crypto.randomUUID(),
    slug: 'job',
    name: 'Job',
    config: {
      schedule: { type: 'crontab', value: '0 * * * *' },
      checkin_margin: 1,
      max_runtime: 30,
      timezone: 'UTC',
      failure_issue_threshold: 1,
      recovery_threshold: 1,
    },
    is_muted: false,
    created_at: '2026-01-01T00:00:00Z',
    environments,
  };
}

describe('monitorHealth', () => {
  it('is waiting before the first check-in', () => {
    expect(monitorHealth(monitor([]))).toBe('waiting');
    expect(monitorHealth(monitor([env({ status: 'active' })]))).toBe('waiting');
  });

  it('is failing when any environment has an open incident', () => {
    expect(
      monitorHealth(monitor([env({ status: 'ok' }), env({ status: 'error' })])),
    ).toBe('failing');
  });

  it('is ok when the environments are healthy', () => {
    expect(
      monitorHealth(
        monitor([env({ status: 'ok' }), env({ status: 'active' })]),
      ),
    ).toBe('ok');
  });
});

describe('lastCheckIn and nextCheckIn', () => {
  const m = monitor([
    env({
      last_checkin: '2026-01-01T05:00:00Z',
      next_checkin: '2026-01-01T07:00:00Z',
    }),
    env({
      last_checkin: '2026-01-01T06:00:00Z',
      next_checkin: '2026-01-01T06:30:00Z',
    }),
    env({}),
  ]);

  it('is the most recent check-in across environments', () => {
    expect(lastCheckIn(m)).toBe('2026-01-01T06:00:00Z');
  });

  it('is the soonest expected check-in across environments', () => {
    expect(nextCheckIn(m)).toBe('2026-01-01T06:30:00Z');
  });

  it('is null without check-ins', () => {
    expect(lastCheckIn(monitor([]))).toBeNull();
    expect(nextCheckIn(monitor([]))).toBeNull();
  });
});
