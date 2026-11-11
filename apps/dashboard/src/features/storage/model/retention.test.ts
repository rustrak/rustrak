import type {
  CleanupCounts,
  ProjectStorage,
  RetentionResponse,
  RetentionSettings,
  StorageAge,
} from '@rustrak/client';
import { describe, expect, it } from 'vitest';
import {
  bucketGoes,
  deletesAnything,
  elapsed,
  estimateBytes,
  estimateSize,
  impactOf,
  nextReadIn,
  presetsWith,
  removedParts,
  summarize,
  windowsFor,
} from './retention';

const forever = { events_days: null, transactions_days: null, logs_days: null };

const settings: RetentionSettings = {
  defaults: { events_days: 90, transactions_days: 30, logs_days: null },
  overrides: [
    { project_id: 2, events_days: 365, transactions_days: null, logs_days: 7 },
  ],
  frequency: 'daily',
  hour: 3,
  weekday: 0,
};

const keepAll: RetentionSettings = {
  ...settings,
  defaults: forever,
  overrides: [],
};

const project = (patch: Partial<ProjectStorage>): ProjectStorage => ({
  project_id: 1,
  project_name: 'api',
  events_count: 0,
  transactions_count: 0,
  spans_count: 0,
  logs_count: 0,
  source_maps_count: 0,
  estimated_bytes: 0,
  events_bytes: 0,
  transactions_bytes: 0,
  logs_bytes: 0,
  ...patch,
});

const none = { counts: [0, 0, 0, 0, 0, 0, 0], oldest: null };

// Project 1: 10 errors at 100 bytes each, half under a week, half over a
// year; 4 logs at 50 bytes, all 30–90 days old. Project 2: 2 logs, 10 bytes
// each, 7–14 days old.
const projects = [
  project({
    project_id: 1,
    project_name: 'api',
    events_count: 10,
    events_bytes: 1_000,
    logs_count: 4,
    logs_bytes: 200,
    estimated_bytes: 1_200,
  }),
  project({
    project_id: 2,
    project_name: 'web',
    logs_count: 2,
    logs_bytes: 20,
    estimated_bytes: 20,
  }),
];
const age: StorageAge = {
  bucket_days: [7, 14, 30, 90, 180, 365],
  projects: [
    {
      project_id: 1,
      events: { counts: [5, 0, 0, 0, 0, 0, 5], oldest: '2024-01-01T00:00:00Z' },
      transactions: none,
      logs: { counts: [0, 0, 0, 4, 0, 0, 0], oldest: '2026-01-01T00:00:00Z' },
    },
    {
      project_id: 2,
      events: none,
      transactions: none,
      logs: { counts: [0, 2, 0, 0, 0, 0, 0], oldest: '2026-03-01T00:00:00Z' },
    },
  ],
};
const sourceMaps = { files: 3, bytes: 500 };

describe('windowsFor', () => {
  it('uses a project override whole, else the defaults', () => {
    expect(windowsFor(settings, 2)).toEqual({
      events_days: 365,
      transactions_days: null,
      logs_days: 7,
    });
    expect(windowsFor(settings, 1)).toEqual(settings.defaults);
  });
});

describe('deletesAnything', () => {
  it('is false only while every kind of every project is kept forever', () => {
    expect(deletesAnything(settings)).toBe(true);
    expect(deletesAnything(keepAll)).toBe(false);
  });
});

describe('presetsWith', () => {
  it('offers the presets, and a saved window outside them', () => {
    expect(presetsWith(30)).toEqual([7, 14, 30, 90, 180, 365, null]);
    expect(presetsWith(45)).toEqual([7, 14, 30, 45, 90, 180, 365, null]);
  });
});

describe('bucketGoes', () => {
  const bounds = [7, 14, 30, 90, 180, 365];

  it('deletes a bucket wholly older than the window', () => {
    // Under 7 days, and 7–14 days, against a 7-day window.
    expect(bucketGoes(0, 7, bounds)).toBe(0);
    expect(bucketGoes(1, 7, bounds)).toBe(1);
    expect(bucketGoes(6, null, bounds)).toBe(0);
  });

  it('deletes the share of a bucket past a window inside it', () => {
    // 30–90 days against 45: the oldest 45 of its 60 days go.
    expect(bucketGoes(3, 45, bounds)).toBe(0.75);
    // Over a year has no end: past 500 days, nothing is known to go.
    expect(bucketGoes(6, 500, bounds)).toBe(0);
    expect(bucketGoes(6, 365, bounds)).toBe(1);
  });
});

describe('summarize', () => {
  const data = { projects, age, sourceMaps, settings };

  it('weighs each kind once, source maps included', () => {
    const s = summarize(data);
    expect(s.kinds.events).toEqual({ bytes: 1_000, rows: 10 });
    expect(s.kinds.logs).toEqual({ bytes: 220, rows: 6 });
    expect(s.kinds.transactions).toEqual({ bytes: 0, rows: 0 });
    expect(s.sourceMaps).toEqual(sourceMaps);
    expect(s.bytes).toBe(1_720);
  });

  it('prices each age bucket at the project\u2019s average row', () => {
    const s = summarize(data);
    expect(s.buckets[0]?.events).toBe(500);
    expect(s.buckets[6]?.events).toBe(500);
  });

  it('marks what the next cleanup deletes, per project\u2019s own windows', () => {
    const s = summarize(data);
    // Project 1's errors over a year (90-day window) and project 2's 7–14
    // day logs (its 7-day override) go; project 1's logs are kept forever.
    expect(s.buckets[6]?.goes.events).toBe(500);
    expect(s.buckets[1]?.goes.logs).toBe(20);
    expect(s.buckets[3]?.goes.logs).toBe(0);
    expect(s.frees).toBe(520);
  });

  it('counts a custom window inside a bucket in proportion', () => {
    const s = summarize({
      ...data,
      settings: {
        ...settings,
        defaults: { ...settings.defaults, logs_days: 45 },
      },
    });
    // Project 1's 200 bytes of 30–90 day logs, three quarters past 45 days.
    expect(s.buckets[3]?.goes.logs).toBe(150);
  });

  it('ranks projects by size, with their own retention when they have one', () => {
    const s = summarize(data);
    expect(s.projects.map((p) => [p.id, p.bytes, p.custom])).toEqual([
      [1, 1_200, false],
      [2, 20, true],
    ]);
    expect(s.projects[0]?.share).toBeCloseTo(1_200 / 1_220);
  });
});

describe('estimateSize', () => {
  const counts = {
    events: 3,
    transactions: 0,
    spans: 0,
    logs: 3,
    issues_removed: 0,
    source_maps: 0,
  };

  it('is not guessed before the figures to price it are in', () => {
    // The dialogs open before the storage figures arrive; a size of 0 B would
    // read as "deletes nothing worth mentioning".
    expect(estimateSize(counts, null)).toBe('…');
  });

  it('adds what is freed besides the rows', () => {
    expect(estimateSize(counts, projects, 614)).toBe('1.0 KB');
  });
});

describe('estimateBytes', () => {
  it('prices counted rows at the instance\u2019s average row per kind', () => {
    // Errors average 100 bytes, logs (220 over 6) about 36.7.
    expect(
      estimateBytes(
        {
          events: 3,
          transactions: 0,
          spans: 0,
          logs: 3,
          issues_removed: 0,
          source_maps: 0,
        },
        projects,
      ),
    ).toBe(410);
  });
});

describe('removedParts', () => {
  it('names what was removed, leaving out the zeros', () => {
    const counts: CleanupCounts = {
      events: 2,
      transactions: 0,
      spans: 0,
      logs: 3,
      issues_removed: 1,
      source_maps: 4,
    };
    expect(removedParts(counts, (key) => key)).toEqual([
      'unit.errors',
      'unit.logs',
      'unit.emptyIssues',
      'unit.sourceMaps',
    ]);
  });
});

describe('nextReadIn', () => {
  const now = Date.parse('2026-01-02T02:00:00Z');
  const state = (patch: Partial<RetentionResponse>): RetentionResponse => ({
    settings,
    next_run_at: '2026-01-02T03:00:00Z',
    last_run: null,
    ...patch,
  });

  it('waits for nothing when no run is scheduled', () => {
    expect(nextReadIn(state({ next_run_at: null }), now)).toBeNull();
  });

  it('reads again when the next run is due', () => {
    expect(nextReadIn(state({}), now)).toBe(3_600_000);
  });

  it('never reads while everything is kept forever', () => {
    expect(
      nextReadIn(state({ settings: keepAll, next_run_at: null }), now),
    ).toBeNull();
  });
});

describe('elapsed', () => {
  const start = '2026-10-23T10:00:00Z';
  const at = (seconds: number) => Date.parse(start) + seconds * 1000;

  it('reads as minutes and padded seconds', () => {
    expect(elapsed(start, at(0))).toBe('0:00');
    expect(elapsed(start, at(7))).toBe('0:07');
    expect(elapsed(start, at(125))).toBe('2:05');
  });

  it('never goes below zero when the clocks disagree', () => {
    expect(elapsed(start, at(-3))).toBe('0:00');
  });

  it('starts from zero without a start', () => {
    expect(elapsed(null, at(42))).toBe('0:00');
  });
});

describe('impactOf', () => {
  it('is the data once in', () => {
    expect(impactOf({ data: 3, isError: false, isFetching: true })).toBe(3);
  });

  it('is failed once a read failed and nothing retries it', () => {
    expect(
      impactOf({ data: undefined, isError: true, isFetching: false }),
    ).toBe('failed');
  });

  it('is counting while a retry runs', () => {
    expect(
      impactOf({ data: undefined, isError: true, isFetching: true }),
    ).toBeNull();
  });
});
