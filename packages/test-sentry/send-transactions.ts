/**
 * Sends rich performance transactions to a local Rustrak server.
 *
 * Builds the Sentry envelope by hand (instead of going through @sentry/node)
 * so each transaction carries a realistic span tree, web-vital measurements,
 * tags, request and user context — exactly the fields the Performance detail
 * page renders. The SDK path with `defaultIntegrations: false` emits empty
 * `spans`, which is why the detail view looked bare.
 *
 * Usage: tsx packages/test-sentry/send-transactions.ts [dsn]
 */
import { randomBytes } from 'node:crypto';
import { DSN } from './src/env.js';

const url = new URL(process.argv[2] ?? DSN);
const SENTRY_KEY = url.username;
const PROJECT_ID = url.pathname.replace(/\//g, '');
const ENDPOINT = `${url.protocol}//${url.host}/api/${PROJECT_ID}/envelope/`;

const hex = (bytes: number) => randomBytes(bytes).toString('hex');

interface SpanDef {
  op: string;
  description: string;
  /** ms from the transaction start. */
  offsetMs: number;
  durationMs: number;
  /** Index of the parent span in the list, or omit for the root segment. */
  parent?: number;
  status?: string;
  data?: Record<string, unknown>;
}

interface TxnDef {
  name: string;
  op: string;
  platform: string;
  durationMs: number;
  status?: string;
  spans: SpanDef[];
  measurements?: Record<string, { value: number; unit: string }>;
  tags?: Record<string, string>;
  request?: Record<string, unknown>;
  user?: Record<string, unknown>;
}

function buildEnvelope(txn: TxnDef): string {
  const eventId = hex(16);
  const traceId = hex(16);
  const rootSpanId = hex(8);
  const end = Date.now() / 1000;
  const start = end - txn.durationMs / 1000;

  // First pass: assign a span_id to every span so children can reference parents.
  const ids = txn.spans.map(() => hex(8));

  const spans = txn.spans.map((s, i) => ({
    span_id: ids[i],
    parent_span_id: s.parent != null ? ids[s.parent] : rootSpanId,
    trace_id: traceId,
    op: s.op,
    description: s.description,
    start_timestamp: start + s.offsetMs / 1000,
    timestamp: start + (s.offsetMs + s.durationMs) / 1000,
    status: s.status ?? 'ok',
    ...(s.data ? { data: s.data } : {}),
  }));

  const event = {
    event_id: eventId,
    type: 'transaction',
    transaction: txn.name,
    platform: txn.platform,
    environment: 'production',
    release: '1.0.0',
    server_name: 'web-1',
    start_timestamp: start,
    timestamp: end,
    contexts: {
      trace: {
        trace_id: traceId,
        span_id: rootSpanId,
        op: txn.op,
        status: txn.status ?? 'ok',
      },
    },
    spans,
    ...(txn.measurements ? { measurements: txn.measurements } : {}),
    ...(txn.tags ? { tags: txn.tags } : {}),
    ...(txn.request ? { request: txn.request } : {}),
    ...(txn.user ? { user: txn.user } : {}),
    sdk: { name: 'rustrak.test-sentry', version: '1.0.0' },
  };

  const payload = JSON.stringify(event);
  const header = JSON.stringify({ event_id: eventId, dsn: DSN });
  const itemHeader = JSON.stringify({
    type: 'transaction',
    length: Buffer.byteLength(payload),
  });
  return `${header}\n${itemHeader}\n${payload}\n`;
}

const TRANSACTIONS: TxnDef[] = [
  {
    name: 'POST /api/checkout',
    op: 'http.server',
    platform: 'node',
    durationMs: 1240,
    tags: { browser: 'Chrome', region: 'eu-west-1' },
    request: {
      method: 'POST',
      url: 'https://shop.example.com/api/checkout',
    },
    user: { id: '42', email: 'buyer@example.com' },
    spans: [
      {
        op: 'db.query',
        description: 'SELECT * FROM carts WHERE id = $1',
        offsetMs: 15,
        durationMs: 120,
        data: { 'db.system': 'postgresql' },
      },
      {
        op: 'http.client',
        description: 'POST https://api.stripe.com/v1/charges',
        offsetMs: 150,
        durationMs: 830,
        data: { 'http.response.status_code': 200 },
      },
      {
        op: 'db.query',
        description: 'INSERT INTO orders (...) VALUES (...)',
        offsetMs: 150,
        durationMs: 60,
        parent: 1,
      },
      {
        op: 'cache.put',
        description: 'redis SET order:9182',
        offsetMs: 1000,
        durationMs: 12,
      },
      {
        op: 'db.query',
        description: 'UPDATE carts SET status = $1',
        offsetMs: 1020,
        durationMs: 200,
      },
    ],
  },
  {
    name: 'GET /checkout',
    op: 'pageload',
    platform: 'javascript',
    durationMs: 2350,
    tags: { browser: 'Firefox', 'device.family': 'Desktop' },
    measurements: {
      lcp: { value: 1850, unit: 'millisecond' },
      fcp: { value: 920, unit: 'millisecond' },
      ttfb: { value: 340, unit: 'millisecond' },
      cls: { value: 0.04, unit: 'none' },
      inp: { value: 180, unit: 'millisecond' },
    },
    spans: [
      {
        op: 'browser.request',
        description: 'Initial document request',
        offsetMs: 0,
        durationMs: 340,
      },
      {
        op: 'resource.script',
        description: '/_next/static/chunks/main.js',
        offsetMs: 360,
        durationMs: 480,
      },
      {
        op: 'resource.css',
        description: '/_next/static/css/app.css',
        offsetMs: 360,
        durationMs: 120,
      },
      {
        op: 'ui.render',
        description: 'Hydrate <CheckoutPage>',
        offsetMs: 900,
        durationMs: 1300,
      },
    ],
  },
  {
    name: 'POST /api/payments/process',
    op: 'http.server',
    platform: 'node',
    durationMs: 3420,
    status: 'internal_error',
    tags: { region: 'us-east-1' },
    spans: [
      {
        op: 'db.query',
        description: 'SELECT * FROM payment_methods WHERE user_id = $1',
        offsetMs: 20,
        durationMs: 90,
      },
      {
        op: 'http.client',
        description: 'POST https://gateway.bank.example/charge',
        offsetMs: 120,
        durationMs: 3200,
        status: 'deadline_exceeded',
        data: { 'http.response.status_code': 504 },
      },
    ],
  },
  {
    name: 'GET /api/products',
    op: 'http.server',
    platform: 'python',
    durationMs: 88,
    spans: [
      {
        op: 'db.query',
        description: 'SELECT * FROM products LIMIT 50',
        offsetMs: 10,
        durationMs: 64,
      },
    ],
  },
  {
    name: 'GET /api/search',
    op: 'http.server',
    platform: 'node',
    durationMs: 520,
    spans: [
      {
        op: 'cache.get',
        description: 'redis GET search:popular',
        offsetMs: 5,
        durationMs: 8,
      },
      {
        op: 'db.query',
        description: 'SELECT ... FROM products WHERE name ILIKE $1',
        offsetMs: 20,
        durationMs: 470,
      },
    ],
  },
];

async function main() {
  console.log(`Sending ${TRANSACTIONS.length} transactions to ${ENDPOINT}\n`);

  for (const txn of TRANSACTIONS) {
    const body = buildEnvelope(txn);
    const res = await fetch(ENDPOINT, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/x-sentry-envelope',
        'X-Sentry-Auth': `Sentry sentry_key=${SENTRY_KEY}, sentry_version=7`,
      },
      body,
    });
    if (!res.ok) process.exitCode = 1;
    const status = res.ok ? '✓' : '✗';
    console.log(
      `${status} ${txn.name} — ${txn.durationMs}ms, ${txn.spans.length} spans (HTTP ${res.status})`,
    );
  }

  console.log('\nDone. Open the Performance tab and click a transaction.');
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
