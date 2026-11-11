import { describe, expect, it } from 'vitest';
import {
  agentPeriodParam,
  defaultSelectedSpanId,
  fillBuckets,
  isNothingYet,
  resolveAgentFilters,
} from './filters';

describe('isNothingYet', () => {
  const none = { agent_runs: 0, llm_calls: 0, tool_calls: 0 };
  const day = resolveAgentFilters({});
  const allTime = resolveAgentFilters({ period: 'all' });

  it('is the setup screen only when nothing has ever arrived', () => {
    expect(isNothingYet(none, day, [])).toBe(true);
    expect(isNothingYet(none, allTime, ['production'])).toBe(true);
  });

  it('is an empty window, not setup, once a span has named an environment', () => {
    expect(isNothingYet(none, day, ['production'])).toBe(false);
    expect(
      isNothingYet(none, resolveAgentFilters({ environment: 'prod' }), []),
    ).toBe(false);
    expect(isNothingYet({ ...none, tool_calls: 1 }, allTime, [])).toBe(false);
  });
});

describe('fillBuckets', () => {
  const hour = (h: number) => new Date(Date.UTC(2026, 0, 1, h)).toISOString();
  const point = (h: number, value: number) => ({ bucket: hour(h), value });

  it('puts the empty buckets back between and after the busy ones', () => {
    const filled = fillBuckets(
      [point(9, 2), point(11, 5)],
      1,
      undefined,
      (bucket) => ({ bucket, value: 0 }),
      Date.UTC(2026, 0, 1, 12, 30),
    );
    expect(filled.map((p) => p.value)).toEqual([2, 0, 5, 0]);
    expect(filled[1]?.bucket).toBe(hour(10));
  });

  it('starts at the window, not at the first busy bucket', () => {
    const filled = fillBuckets(
      [point(11, 5)],
      1,
      3,
      (bucket) => ({ bucket, value: 0 }),
      Date.UTC(2026, 0, 1, 12, 30),
    );
    // 09:30 is the window's start, in the 09:00 bucket.
    expect(filled.map((p) => p.value)).toEqual([0, 0, 5, 0]);
  });

  it('fills only up to now, but keeps a busy bucket past it', () => {
    const ahead = '9999-01-01T00:00:00.000Z';
    const filled = fillBuckets(
      [point(11, 5), { bucket: ahead, value: 1 }],
      1,
      undefined,
      (bucket) => ({ bucket, value: 0 }),
      Date.UTC(2026, 0, 1, 12, 30),
    );
    // One point for the clock far ahead, not an empty bucket per hour to it.
    expect(filled.map((p) => p.bucket)).toEqual([hour(11), hour(12), ahead]);
    expect(filled.map((p) => p.value)).toEqual([5, 0, 1]);
  });

  it('keeps the newest runs when the browser clock runs behind', () => {
    const filled = fillBuckets(
      [point(9, 2), point(12, 5)],
      1,
      undefined,
      (bucket) => ({ bucket, value: 0 }),
      Date.UTC(2026, 0, 1, 10, 30),
    );
    expect(filled.map((p) => p.bucket)).toEqual([hour(9), hour(10), hour(12)]);
    expect(filled.map((p) => p.value)).toEqual([2, 0, 5]);
  });
});

describe('resolveAgentFilters', () => {
  it('widens the bucket with the window', () => {
    // A 30-day window bucketed hourly is 720 points on a chart a few hundred
    // pixels wide — noise, not detail.
    expect(resolveAgentFilters({ period: '24h' }).intervalHours).toBe(1);
    expect(resolveAgentFilters({ period: '30d' }).intervalHours).toBe(24);
  });

  it('reads the last 24 hours unless asked otherwise', () => {
    // All time scans every span a project ever sent; on a busy one that
    // outlasts the client's timeout. A stale or hand-edited period gets the
    // default too.
    for (const period of [undefined, 'last-tuesday']) {
      const filters = resolveAgentFilters({ period });
      expect(filters.period).toBe('24h');
      expect(filters.periodHours).toBe(24);
    }
  });

  it('reads all time only when asked to', () => {
    const filters = resolveAgentFilters({ period: 'all' });

    expect(filters.period).toBeUndefined();
    expect(filters.periodHours).toBeUndefined();
    expect(filters.intervalHours).toBe(24);
  });

  it('writes the default window as no period, and all time as all', () => {
    expect(agentPeriodParam('24h')).toBeUndefined();
    expect(agentPeriodParam('7d')).toBe('7d');
    expect(agentPeriodParam(undefined)).toBe('all');
  });

  it('treats an empty environment as no filter', () => {
    // `?environment=` is what a cleared dropdown produces; it must not become
    // a query for spans whose environment is the empty string.
    expect(
      resolveAgentFilters({ environment: '' }).environment,
    ).toBeUndefined();
  });
});

describe('defaultSelectedSpanId', () => {
  const span = (
    id: string,
    gen_ai_operation_type: string | null,
    start_timestamp: string,
  ) => ({ id, gen_ai_operation_type, start_timestamp });

  it('opens on the first LLM call, not on the agent root', () => {
    // The agent root mostly restates the trace header. The first generation
    // is where the prompt and the answer are.
    const chosen = defaultSelectedSpanId([
      span('agent', 'agent', '2026-08-13T10:00:00Z'),
      span('llm', 'ai_client', '2026-08-13T10:00:02Z'),
    ]);

    expect(chosen).toBe('llm');
  });

  it('picks the earliest LLM call when there are several', () => {
    const chosen = defaultSelectedSpanId([
      span('late', 'ai_client', '2026-08-13T10:00:09Z'),
      span('early', 'ai_client', '2026-08-13T10:00:01Z'),
    ]);

    expect(chosen).toBe('early');
  });

  it('falls back to any AI span when the trace made no model call', () => {
    // A pure tool-calling trace still has something worth opening.
    const chosen = defaultSelectedSpanId([
      span('plain', null, '2026-08-13T10:00:00Z'),
      span('tool', 'tool', '2026-08-13T10:00:01Z'),
    ]);

    expect(chosen).toBe('tool');
  });

  it('is undefined for an empty trace rather than throwing', () => {
    expect(defaultSelectedSpanId([])).toBeUndefined();
  });
  it('orders deterministically when a span has no timestamp', () => {
    // `Date.parse(null ?? '')` is NaN, and a comparator returning NaN leaves
    // the order implementation-defined — the default selection would then
    // depend on the order rows came back in.
    const withoutTimestamp = {
      id: 'no-ts',
      gen_ai_operation_type: 'ai_client',
      start_timestamp: null,
    };
    const early = span('early', 'ai_client', '2026-08-13T10:00:01Z');

    expect(defaultSelectedSpanId([withoutTimestamp, early])).toBe('early');
    expect(defaultSelectedSpanId([early, withoutTimestamp])).toBe('early');
  });
});
