import { describe, expect, it } from 'vitest';
import { type ClockSpan, waterfallRows } from './waterfall';

const span = (over: Partial<ClockSpan> & { id: string }): ClockSpan => ({
  startMs: null,
  endMs: null,
  ...over,
});

describe('waterfallRows', () => {
  it('links children by span id and sends orphans to the given parent', () => {
    const rows = waterfallRows(
      [
        span({ id: 'root', spanId: 'r', startMs: 0, endMs: 100 }),
        span({
          id: 'a',
          spanId: 'a',
          parentSpanId: 'r',
          startMs: 10,
          endMs: 20,
        }),
        span({ id: 'b', parentSpanId: 'gone', startMs: 30, endMs: 40 }),
      ],
      { origin: 0, orphanParentId: 'root' },
    );

    expect(rows.map((r) => [r.id, r.parentId])).toEqual([
      ['root', null],
      ['a', 'root'],
      ['b', 'root'],
    ]);
  });

  it('keeps a span without timestamps as an unfinished row at its parent', () => {
    const rows = waterfallRows(
      [
        span({ id: 'p', spanId: 'p', startMs: 50, endMs: 90 }),
        span({ id: 'x', parentSpanId: 'p' }),
        span({ id: 'y' }),
      ],
      { origin: 5, orphanParentId: null },
    );

    expect(rows[1]).toMatchObject({
      startMs: 50,
      endMs: 50,
      unfinished: true,
      selfMs: 0,
    });
    expect(rows[2]).toMatchObject({ startMs: 5, endMs: 5, unfinished: true });
    expect(rows[0]?.unfinished).toBe(false);
  });

  it('places a span with only one timestamp at it, unfinished', () => {
    const [onlyEnd, onlyStart] = waterfallRows(
      [span({ id: 'e', endMs: 70 }), span({ id: 's', startMs: 60 })],
      { origin: 0, orphanParentId: null },
    );

    expect(onlyEnd).toMatchObject({ startMs: 70, endMs: 70, unfinished: true });
    expect(onlyStart).toMatchObject({
      startMs: 60,
      endMs: 60,
      unfinished: true,
    });
  });

  it('counts self time once for overlapping children, or takes the SDK value', () => {
    const rows = waterfallRows(
      [
        span({ id: 'r', spanId: 'r', startMs: 0, endMs: 1000 }),
        span({ id: 'a', parentSpanId: 'r', startMs: 100, endMs: 500 }),
        span({ id: 'b', parentSpanId: 'r', startMs: 100, endMs: 500 }),
        span({
          id: 'c',
          parentSpanId: 'r',
          startMs: 500,
          endMs: 600,
          exclusiveMs: 42,
        }),
      ],
      { origin: 0, orphanParentId: null },
    );

    expect(rows[0]?.selfMs).toBe(500);
    expect(rows[3]?.selfMs).toBe(42);
  });
});
