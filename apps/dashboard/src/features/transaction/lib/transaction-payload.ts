import type { TransactionDetail } from '@rustrak/client';
import type { Span, TraceContext } from '@/features/transaction/model/span';
import { parseEpochSeconds } from '@/shared/lib/protocol-timestamp';

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
  txn: TransactionDetail,
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
