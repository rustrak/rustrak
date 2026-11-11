import type { TransactionDetailResponse } from '@rustrak/client';
import { describe, expect, it } from 'vitest';
import {
  readMeasurements,
  readTransactionPayload,
  waterfallSpans,
} from './transaction-payload';

function txn(
  over: Partial<TransactionDetailResponse> = {},
): TransactionDetailResponse {
  return {
    id: 't1',
    transaction_name: 'GET /',
    duration_ms: 12,
    timestamp: '2026-01-01T00:00:10.000Z',
    start_timestamp: '2026-01-01T00:00:00.000Z',
    data: {},
    ...over,
  } as unknown as TransactionDetailResponse;
}

describe('readTransactionPayload', () => {
  it('reads the trace context, its op and its status', () => {
    const payload = readTransactionPayload(
      txn({
        data: { contexts: { trace: { op: 'http.server', status: 'ok' } } },
      }),
    );

    expect(payload.op).toBe('http.server');
    expect(payload.status).toBe('ok');
    expect(payload.trace).toEqual({ op: 'http.server', status: 'ok' });
  });

  it('ignores an op or status that is not a string', () => {
    const payload = readTransactionPayload(
      txn({ data: { contexts: { trace: { op: 7, status: null } } } }),
    );

    expect(payload.op).toBeUndefined();
    expect(payload.status).toBeUndefined();
  });

  it('treats a missing trace context as absent rather than empty', () => {
    expect(readTransactionPayload(txn()).trace).toBeUndefined();
  });

  it('rejects an array where an object is expected', () => {
    // `typeof [] === 'object'`, so a payload with `tags: []` would otherwise
    // render an empty key/value panel instead of nothing.
    expect(readTransactionPayload(txn({ data: { tags: [] } })).tags).toBeNull();
  });

  it('reads spans only when the payload holds a list', () => {
    expect(
      readTransactionPayload(txn({ data: { spans: [{}, {}] } })).spans,
    ).toHaveLength(2);
    expect(
      readTransactionPayload(txn({ data: { spans: 'nope' } })).spans,
    ).toEqual([]);
  });

  it('normalizes mixed numeric and RFC3339 span timestamps', () => {
    const payload = readTransactionPayload(
      txn({
        data: {
          spans: [
            {
              span_id: 's1',
              start_timestamp: 1000,
              timestamp: '1970-01-01T00:16:42.000Z',
            },
          ],
        },
      }),
    );

    expect(payload.spans).toEqual([
      {
        span_id: 's1',
        start_timestamp: 1000,
        timestamp: 1002,
      },
    ]);
  });

  it('drops invalid span timestamps instead of exposing NaN arithmetic', () => {
    const payload = readTransactionPayload(
      txn({
        data: {
          spans: [
            {
              span_id: 's1',
              start_timestamp: Number.NaN,
              timestamp: 'not-a-timestamp',
            },
          ],
        },
      }),
    );

    expect(payload.spans).toEqual([
      {
        span_id: 's1',
        start_timestamp: undefined,
        timestamp: undefined,
      },
    ]);
  });

  // Captured from sentry-types 0.49.2, which serializes a span's two ends with
  // different encodings: ts_seconds_float and ts_rfc3339_opt.
  it('reads a real Rust-SDK span into arithmetic that yields a duration', () => {
    const payload = readTransactionPayload(
      txn({
        timestamp: '2025-09-04T15:33:20.800Z',
        start_timestamp: '2025-09-04T15:33:20.500Z',
        data: {
          start_timestamp: 1757000000.5,
          timestamp: '2025-09-04T15:33:20.8Z',
          spans: [
            {
              op: 'db.sql.query',
              span_id: 'f796eed91584d618',
              start_timestamp: 1757000000.5,
              timestamp: '2025-09-04T15:33:20.8Z',
              trace_id: 'db542d8976bbdc31d5940a8bfe604128',
            },
          ],
        },
      }),
    );

    const [span] = payload.spans;
    const durationMs =
      ((span.timestamp as number) - (span.start_timestamp as number)) * 1000;

    expect(durationMs).toBeCloseTo(300, 3);
  });

  it('prefers the payload epoch seconds over the row timestamps', () => {
    const payload = readTransactionPayload(
      txn({ data: { start_timestamp: 1000, timestamp: 1002 } }),
    );

    expect(payload.transactionStart).toBe(1000);
    expect(payload.transactionEnd).toBe(1002);
  });

  it('falls back to the row timestamps when the payload has none', () => {
    // The SDK sends epoch seconds; the row stores ISO. The waterfall's clock
    // is in seconds, so a fallback has to be converted, not passed through.
    const payload = readTransactionPayload(txn());

    expect(payload.transactionStart).toBe(
      Date.parse('2026-01-01T00:00:00.000Z') / 1000,
    );
    expect(payload.transactionEnd).toBe(
      Date.parse('2026-01-01T00:00:10.000Z') / 1000,
    );
  });

  it('falls back to the end timestamp when the row has no start', () => {
    const payload = readTransactionPayload(txn({ start_timestamp: null }));

    expect(payload.transactionStart).toBe(
      Date.parse('2026-01-01T00:00:10.000Z') / 1000,
    );
  });
});

describe('waterfallSpans', () => {
  it('puts the transaction at the root and its spans on its clock', () => {
    const payload = readTransactionPayload(
      txn({
        data: {
          start_timestamp: 100,
          timestamp: 100.5,
          contexts: { trace: { span_id: 'r', op: 'http.server' } },
          spans: [
            {
              span_id: 'a',
              parent_span_id: 'r',
              op: 'db',
              description: 'SELECT 1',
              start_timestamp: 100.1,
              timestamp: 100.2,
            },
            // Its parent was never sent: it hangs off the root, not nowhere.
            {
              span_id: 'b',
              parent_span_id: 'gone',
              start_timestamp: 100.3,
              timestamp: 100.4,
              status: 'internal_error',
            },
          ],
        },
      }),
    );

    const rows = waterfallSpans(payload, 'GET /');
    expect(rows[0]).toMatchObject({
      id: 'r',
      parentId: null,
      op: 'http.server',
      description: 'GET /',
      startMs: 0,
    });
    expect(rows[0]?.endMs).toBeCloseTo(500);
    expect(rows[1]).toMatchObject({ id: 'a', parentId: 'r', op: 'db' });
    expect(rows[1]?.startMs).toBeCloseTo(100);
    expect(rows[2]).toMatchObject({ parentId: 'r', status: 'internal_error' });
    // Self time: its own duration less its children's, 500 - 100 - 100.
    expect(rows[0]?.selfMs).toBeCloseTo(300);
    expect(rows[1]?.selfMs).toBeCloseTo(100);
  });

  it("takes a span's self time from the SDK when it sent one", () => {
    const payload = readTransactionPayload(
      txn({
        data: {
          start_timestamp: 100,
          timestamp: 101,
          spans: [
            {
              span_id: 'a',
              start_timestamp: 100,
              timestamp: 100.5,
              exclusive_time: 42,
            },
          ],
        },
      }),
    );
    expect(waterfallSpans(payload, 'GET /')[1]?.selfMs).toBe(42);
  });

  it('counts time children share only once', () => {
    const payload = readTransactionPayload(
      txn({
        data: {
          start_timestamp: 100,
          timestamp: 101,
          contexts: { trace: { span_id: 'r' } },
          // Two in parallel over the same 400 ms, one after them for 100 ms.
          spans: [
            { span_id: 'a', start_timestamp: 100.1, timestamp: 100.5 },
            { span_id: 'b', start_timestamp: 100.1, timestamp: 100.5 },
            { span_id: 'c', start_timestamp: 100.5, timestamp: 100.6 },
          ],
        },
      }),
    );
    expect(waterfallSpans(payload, 'GET /')[0]?.selfMs).toBeCloseTo(500);
  });

  it('keeps a span with no clock as an unfinished row', () => {
    const payload = readTransactionPayload(
      txn({ data: { spans: [{ span_id: 'x' }] } }),
    );
    const rows = waterfallSpans(payload, 'GET /');
    expect(rows).toHaveLength(2);
    expect(rows[1]).toMatchObject({ id: 'x', startMs: 0, unfinished: true });
  });
});

describe('readMeasurements', () => {
  it('rates the web vitals first and keeps the rest', () => {
    const rows = readMeasurements({
      custom: { value: 3, unit: 'none' },
      lcp: { value: 4200, unit: 'millisecond' },
      cls: { value: 0.05, unit: '' },
      junk: 'x',
    });
    expect(rows.map((r) => r.key)).toEqual(['lcp', 'cls', 'custom']);
    expect(rows[0]).toMatchObject({ vital: true, rating: 'poor' });
    expect(rows[1]).toMatchObject({ rating: 'good' });
    expect(rows[2]).toMatchObject({ vital: false, rating: null });
  });

  it('never reads an inherited name as a vital', () => {
    const [row] = readMeasurements({ constructor: { value: 1 } });
    expect(row).toMatchObject({ vital: false, rating: null });
  });
});
