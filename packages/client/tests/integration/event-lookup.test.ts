import { HttpResponse, http } from 'msw';
import { describe, expect, it } from 'vitest';
import { RustrakClient } from '../../src/client.js';
import { expectErr, expectOk } from '../helpers/result.js';
import { server } from '../setup.js';

const endpoint = 'http://localhost:8080/api/projects/:projectId/events/lookup';
const client = () =>
  new RustrakClient({ baseUrl: 'http://localhost:8080', token: 'test-token' });
const event = {
  id: '523e4567-e89b-42d3-a456-426614174000',
  event_id: '123e4567-e89b-12d3-a456-426614174000',
  issue_id: '323e4567-e89b-42d3-a456-426614174000',
  title: 'TypeError: lookup proof',
  timestamp: '2026-09-30T12:00:00Z',
  level: 'error',
  platform: 'javascript',
  release: '1',
  environment: 'test',
  event_type: 'error',
};

describe('events.lookup()', () => {
  it('looks up a user across issues and preserves pagination', async () => {
    server.use(
      http.get(endpoint, ({ request, params }) => {
        const url = new URL(request.url);
        expect(params.projectId).toBe('1');
        expect(url.searchParams.get('user_id')).toBe('user/&+é');
        expect(url.searchParams.get('request_id')).toBeNull();
        expect(url.searchParams.get('cursor')).toBe('lookup-cursor');
        return HttpResponse.json({
          items: [event],
          has_more: true,
          next_cursor: 'next-page',
        });
      }),
    );
    const page = expectOk(
      await client().events.lookup(1, {
        user_id: 'user/&+é',
        cursor: 'lookup-cursor',
      }),
    );
    expect(page.items).toEqual([event]);
    expect(page.next_cursor).toBe('next-page');
    expect(page.has_more).toBe(true);
  });

  it('looks up an exact request tag and accepts an empty result', async () => {
    server.use(
      http.get(endpoint, ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.get('request_id')).toBe('request=/#&');
        expect(url.searchParams.get('user_id')).toBeNull();
        return HttpResponse.json({ items: [], has_more: false });
      }),
    );
    const page = expectOk(
      await client().events.lookup(1, { request_id: 'request=/#&' }),
    );
    expect(page.items).toEqual([]);
    expect(page.has_more).toBe(false);
  });

  it('rejects invalid options before issuing a request', async () => {
    let calls = 0;
    server.use(
      http.get(endpoint, () => {
        calls++;
        return HttpResponse.json({ items: [], has_more: false });
      }),
    );
    for (const options of [
      {},
      { user_id: '' },
      { user_id: 'u', request_id: 'r' },
      { request_id: 'é'.repeat(101) },
      { user_id: ' u' },
      { user_id: 'u\n' },
      { user_id: 'u', cursor: 'a'.repeat(2049) },
      { user_id: 'u', unknown: 'x' },
    ]) {
      // Runtime callers can bypass TypeScript; the input schema is authoritative.
      // @ts-expect-error Deliberately malformed lookup options.
      expect(expectErr(await client().events.lookup(1, options)).kind).toBe(
        'invalid_request',
      );
    }
    expect(calls).toBe(0);
  });

  it('accepts an identifier at the 200-byte UTF-8 boundary', async () => {
    server.use(
      http.get(endpoint, () =>
        HttpResponse.json({ items: [], has_more: false }),
      ),
    );
    expectOk(await client().events.lookup(1, { user_id: 'é'.repeat(100) }));
  });

  it.each([401, 404, 400])('preserves an HTTP %i failure', async (status) => {
    server.use(
      http.get(endpoint, () =>
        HttpResponse.json(
          { error: { type: 'lookup_error', message: 'Lookup rejected' } },
          { status },
        ),
      ),
    );
    const error = expectErr(await client().events.lookup(1, { user_id: 'u' }));
    expect(error.kind).toBe(
      { 401: 'unauthenticated', 404: 'not_found', 400: 'validation' }[status],
    );
  });

  it('rejects malformed successful responses', async () => {
    server.use(
      http.get(endpoint, () =>
        HttpResponse.json({
          items: [{ ...event, id: 'invalid' }],
          has_more: false,
        }),
      ),
    );
    expect(
      expectErr(await client().events.lookup(1, { request_id: 'r' })).kind,
    ).toBe('invalid_response');
  });
});
