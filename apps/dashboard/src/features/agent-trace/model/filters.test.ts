import { describe, expect, it } from 'vitest';
import {
  agentDashboardSearch,
  defaultSelectedSpanId,
  resolveAgentFilters,
} from './filters';

describe('resolveAgentFilters', () => {
  it('widens the bucket with the window', () => {
    // A 30-day window bucketed hourly is 720 points on a chart a few hundred
    // pixels wide — noise, not detail.
    expect(resolveAgentFilters({ period: '24h' }).intervalHours).toBe(1);
    expect(resolveAgentFilters({ period: '30d' }).intervalHours).toBe(24);
  });

  it('falls back to all time for an unrecognized period', () => {
    // A stale or hand-edited URL should still render something, and showing
    // more data than asked for is the safe direction to fail in.
    const filters = resolveAgentFilters({ period: 'last-tuesday' });

    expect(filters.period).toBeUndefined();
    expect(filters.periodHours).toBeUndefined();
  });

  it('treats an empty environment as no filter', () => {
    // `?environment=` is what a cleared dropdown produces; it must not become
    // a query for spans whose environment is the empty string.
    expect(
      resolveAgentFilters({ environment: '' }).environment,
    ).toBeUndefined();
  });
});

describe('agentDashboardSearch', () => {
  it('keeps the other filter when one changes', () => {
    expect(
      agentDashboardSearch(
        { period: '7d', environment: 'production' },
        { period: '1h' },
      ),
    ).toStrictEqual({ period: '1h', environment: 'production' });
  });

  it('drops a filter set to null rather than writing it empty', () => {
    expect(
      agentDashboardSearch(
        { period: '7d', environment: 'production' },
        { environment: null },
      ),
    ).toStrictEqual({ period: '7d' });
  });

  it('is empty when nothing is filtered', () => {
    // No keys at all, not keys holding `undefined`: the URL stays the clean one.
    expect(agentDashboardSearch({}, {})).toStrictEqual({});
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
