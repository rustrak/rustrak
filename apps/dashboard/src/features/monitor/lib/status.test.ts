import type {
  CheckInResponse,
  MonitorEnvironmentResponse,
  MonitorResponse,
} from '@rustrak/client';
import { describe, expect, it } from 'vitest';
import {
  filterMonitors,
  lastCheckIn,
  lastRun,
  monitorHealth,
  nextCheckIn,
  nextCheckInState,
} from './status';

function env(
  overrides: Partial<MonitorEnvironmentResponse>,
): MonitorEnvironmentResponse {
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

function monitor(environments: MonitorEnvironmentResponse[]): MonitorResponse {
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

describe('lastRun', () => {
  const run = (overrides: Partial<CheckInResponse>): CheckInResponse =>
    ({ status: 'ok', duration_ms: 1000, ...overrides }) as CheckInResponse;

  it('is the newest finished run across environments, not the first listed', () => {
    const m = monitor([
      // `alpha` sorts first, but `beta` ran last.
      env({
        environment: 'alpha',
        recent_checkins: [
          run({ date_added: '2026-01-01T05:00:00Z', duration_ms: 1000 }),
        ],
      }),
      env({
        environment: 'beta',
        recent_checkins: [
          run({ date_added: '2026-01-01T07:00:00Z', status: 'in_progress' }),
          run({ date_added: '2026-01-01T06:00:00Z', duration_ms: 3000 }),
        ],
      }),
    ]);
    expect(lastRun(m)?.duration_ms).toBe(3000);
  });

  it('is undefined before any run finished', () => {
    expect(lastRun(monitor([]))).toBeUndefined();
  });
});

describe('filterMonitors', () => {
  const named = (name: string, status: 'ok' | 'error', muted = false) => ({
    ...monitor([env({ status })]),
    name,
    slug: name.toLowerCase(),
    is_muted: muted,
  });
  const list = [
    named('Backup', 'ok'),
    named('Sync', 'error'),
    named('Audit', 'ok', true),
  ];

  it('puts failing first, then by name', () => {
    expect(filterMonitors(list, '').map((m) => m.name)).toEqual([
      'Sync',
      'Audit',
      'Backup',
    ]);
  });

  it('narrows by text and by state', () => {
    expect(filterMonitors(list, 'back').map((m) => m.name)).toEqual(['Backup']);
    expect(filterMonitors(list, '', 'muted').map((m) => m.name)).toEqual([
      'Audit',
    ]);
  });
});

describe('nextCheckInState', () => {
  const now = Date.parse('2026-01-01T06:00:00Z');
  const ahead = '2026-01-01T06:02:00Z';
  const checkIn = (status: CheckInResponse['status']) =>
    ({ status }) as CheckInResponse;

  it('says the last run was missed, even with the next slot ahead', () => {
    const m = monitor([
      env({ next_checkin: ahead, recent_checkins: [checkIn('missed')] }),
    ]);
    expect(nextCheckInState(m, now)).toBe('missed');
  });

  it('is late past the expected time, and due before it', () => {
    expect(
      nextCheckInState(
        monitor([env({ next_checkin: '2026-01-01T05:59:00Z' })]),
        now,
      ),
    ).toBe('late');
    expect(
      nextCheckInState(
        monitor([
          env({ next_checkin: ahead, recent_checkins: [checkIn('ok')] }),
        ]),
        now,
      ),
    ).toBe('due');
    expect(nextCheckInState(monitor([env({})]), now)).toBe('none');
  });
});
