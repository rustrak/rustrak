import type { TransactionDetailResponse } from '@rustrak/client';
import type { Span, TraceContext } from '@/features/transaction/model/span';
import { parseEpochSeconds } from '@/shared/lib/protocol-timestamp';
import { type WaterfallRow, waterfallRows } from '@/shared/lib/waterfall';

function asObject(value: unknown): Record<string, unknown> | null {
  // `typeof [] === 'object'`, and a list is never one of the key/value panels
  // this reads, so an array is not an object here.
  if (value && typeof value === 'object' && !Array.isArray(value)) {
    return value as Record<string, unknown>;
  }
  return null;
}

/**
 * A moment on the waterfall's clock, which counts seconds.
 *
 * The SDK sends epoch seconds inside the payload; the row stores ISO strings.
 * The fallback has to be converted rather than passed through, or every bar
 * would be placed a thousand times too far along.
 */
function epochSeconds(raw: unknown, fallbackIso: string): number {
  return parseEpochSeconds(raw) ?? Date.parse(fallbackIso) / 1000;
}

/**
 * Normalizes timestamps on a transaction-embedded span to epoch seconds.
 *
 * Sentry SDKs may encode protocol timestamps as either epoch numbers or
 * RFC3339 strings. Keeping that distinction out of the waterfall prevents
 * arithmetic on a string from producing `NaN` durations and geometry.
 */
function readSpan(raw: unknown): Span | null {
  const span = asObject(raw);
  if (!span) return null;

  return {
    ...span,
    start_timestamp: parseEpochSeconds(span.start_timestamp),
    timestamp: parseEpochSeconds(span.timestamp),
  } as Span;
}

function asString(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined;
}

/** Everything the transaction detail page reads out of the stored payload. */
export interface TransactionPayload {
  trace: TraceContext | undefined;
  spans: Span[];
  measurements: Record<string, unknown> | null;
  tags: Record<string, unknown> | null;
  request: Record<string, unknown> | null;
  user: Record<string, unknown> | null;
  /** Epoch seconds, the left edge of the waterfall. */
  transactionStart: number;
  /** Epoch seconds, its right edge. */
  transactionEnd: number;
  op: string | undefined;
  status: string | undefined;
}

/**
 * The stored event payload, narrowed into the shapes the page renders.
 *
 * `data` is whatever the SDK sent, so every read here is a question about an
 * `unknown`. Doing it once, in one place, is what keeps those questions out of
 * the page and provable on their own.
 */
export function readTransactionPayload(
  txn: TransactionDetailResponse,
): TransactionPayload {
  const data = (txn.data ?? {}) as Record<string, unknown>;
  const trace = asObject(asObject(data.contexts)?.trace) ?? undefined;

  return {
    trace: trace as TraceContext | undefined,
    spans: Array.isArray(data.spans)
      ? data.spans.flatMap((span) => {
          const normalized = readSpan(span);
          return normalized ? [normalized] : [];
        })
      : [],
    measurements: asObject(data.measurements),
    tags: asObject(data.tags),
    request: asObject(data.request),
    user: asObject(data.user),

    transactionStart: epochSeconds(
      data.start_timestamp,
      txn.start_timestamp ?? txn.timestamp,
    ),
    transactionEnd: epochSeconds(data.timestamp, txn.timestamp),

    op: asString(trace?.op),
    status: asString(trace?.status),
  };
}

/**
 * The transaction as a tree of rows on its own clock: the transaction itself
 * at the root, named after it, and every span under its parent. A span whose
 * parent was never sent hangs off the root rather than off nothing.
 */
export function waterfallSpans(
  payload: TransactionPayload,
  name: string,
): WaterfallRow[] {
  const start = payload.transactionStart;
  const ms = (seconds: number | null | undefined) =>
    seconds == null ? null : (seconds - start) * 1000;
  const rootId = payload.trace?.span_id ?? 'root';

  return waterfallRows(
    [
      {
        id: rootId,
        spanId: rootId,
        op: payload.op,
        description: name,
        startMs: 0,
        endMs: ms(payload.transactionEnd),
        status: payload.status,
      },
      ...payload.spans.map((span, index) => ({
        id: span.span_id ?? `span-${index}`,
        spanId: span.span_id,
        parentSpanId: span.parent_span_id,
        op: span.op,
        description: span.description,
        startMs: ms(span.start_timestamp),
        endMs: ms(span.timestamp),
        status: span.status,
        exclusiveMs: span.exclusive_time,
      })),
    ],
    { origin: 0, orphanParentId: rootId },
  );
}

/** Web-vital thresholds (good up to, poor from) from web.dev, in ms but CLS. */
const VITALS: Record<string, { good: number; poor: number }> = {
  lcp: { good: 2500, poor: 4000 },
  fcp: { good: 1800, poor: 3000 },
  fid: { good: 100, poor: 300 },
  inp: { good: 200, poor: 500 },
  ttfb: { good: 800, poor: 1800 },
  cls: { good: 0.1, poor: 0.25 },
};

export interface MeasurementRow {
  key: string;
  value: number;
  unit?: string;
  /** One of the web vitals, with a known label and thresholds. */
  vital: boolean;
  rating: 'good' | 'meh' | 'poor' | null;
}

/** The numeric measurements, the web vitals first and rated. */
export function readMeasurements(
  measurements: Record<string, unknown> | null,
): MeasurementRow[] {
  if (!measurements) return [];
  const rows = Object.entries(measurements).flatMap(
    ([key, raw]): MeasurementRow[] => {
      const m = asObject(raw);
      if (!m || typeof m.value !== 'number') return [];
      // Own keys only: `constructor` is not a vital.
      const vital = Object.hasOwn(VITALS, key.toLowerCase())
        ? VITALS[key.toLowerCase()]
        : undefined;
      const value = m.value;
      const rating: MeasurementRow['rating'] = vital
        ? value <= vital.good
          ? 'good'
          : value <= vital.poor
            ? 'meh'
            : 'poor'
        : null;
      return [{ key, value, unit: asString(m.unit), vital: !!vital, rating }];
    },
  );
  return rows.sort((a, b) => Number(b.vital) - Number(a.vital));
}
