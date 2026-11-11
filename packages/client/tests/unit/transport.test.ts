import { HttpResponse, http } from 'msw';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RustrakClient } from '../../src/client.js';
import { expectErr, expectOk } from '../helpers/result.js';
import { server } from '../setup.js';

const BASE = 'http://localhost:8080';
const client = (maxRetries = 2) =>
  new RustrakClient({ baseUrl: BASE, token: 'test-token', maxRetries });

/** Answers `status` and counts the attempts that reached the server. */
function failing(method: 'get' | 'post', path: string, status: number) {
  const seen = { attempts: 0 };
  server.use(
    http[method](`${BASE}${path}`, () => {
      seen.attempts++;
      return HttpResponse.json(
        { error: { type: 'InternalError', message: 'x' } },
        { status },
      );
    }),
  );
  return seen;
}

/** Runs the backoff sleeps without waiting for them. */
async function settle<T>(pending: Promise<T>): Promise<T> {
  let done = false;
  pending.finally(() => {
    done = true;
  });
  while (!done) await vi.advanceTimersByTimeAsync(1000);
  return pending;
}

describe('retries', () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['setTimeout'] });
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('retries an idempotent request up to maxRetries', async () => {
    const seen = failing('get', '/api/projects', 503);
    expect(expectErr(await settle(client(2).projects.list())).kind).toBe(
      'server_error',
    );
    expect(seen.attempts).toBe(3);
  });

  it.each([401, 404, 429, 501])('does not repeat a %i', async (status) => {
    const seen = failing('get', '/api/projects', status);
    expectErr(await settle(client(2).projects.list()));
    expect(seen.attempts).toBe(1);
  });

  it('retries a 408', async () => {
    const seen = failing('get', '/api/projects', 408);
    expectErr(await settle(client(1).projects.list()));
    expect(seen.attempts).toBe(2);
  });

  // A POST that timed out at a proxy may still have created the row.
  it('never repeats a POST', async () => {
    const seen = failing('post', '/api/projects', 503);
    expectErr(await client(2).projects.create({ body: { name: 'once' } }));
    expect(seen.attempts).toBe(1);
  });

  it('retries a GET whose connection failed, then succeeds', async () => {
    let attempts = 0;
    server.use(
      http.get(`${BASE}/health`, () =>
        ++attempts === 1
          ? HttpResponse.error()
          : HttpResponse.json({ status: 'ok' }),
      ),
    );
    expectOk(await settle(client(1).health.liveness()));
    expect(attempts).toBe(2);
  });
});

describe('cancellation', () => {
  // The caller asked for it: not a transport failure, not an `Err`.
  it("rethrows the caller's own abort", async () => {
    server.use(
      http.get(`${BASE}/api/projects`, async () => {
        await new Promise((resolve) => setTimeout(resolve, 50));
        return HttpResponse.json({});
      }),
    );
    const controller = new AbortController();
    const pending = client(0).projects.list({ signal: controller.signal });
    controller.abort();
    await expect(pending).rejects.toMatchObject({ name: 'AbortError' });
  });
});

describe('empty bodies', () => {
  it('reports success for a 204', async () => {
    server.use(
      http.delete(
        `${BASE}/api/projects/1`,
        () => new HttpResponse(null, { status: 204 }),
      ),
    );
    expectOk(await client().projects.delete({ path: { id: 1 } }));
  });
});

describe('headers', () => {
  it('sends the token, JSON Accept and the caller headers', async () => {
    let headers: Headers | undefined;
    server.use(
      http.get(`${BASE}/health/version`, ({ request }) => {
        headers = request.headers;
        return HttpResponse.json({ version: '1.0.0' });
      }),
    );
    await new RustrakClient({
      baseUrl: BASE,
      token: 'secret',
      headers: { 'X-Trace': 'abc' },
    }).health.version();
    expect(headers?.get('authorization')).toBe('Bearer secret');
    expect(headers?.get('accept')).toBe('application/json');
    expect(headers?.get('x-trace')).toBe('abc');
  });
});
