import { HttpResponse, http } from 'msw/http';
import { describe, expect, it } from 'vitest';
import { RustrakClient } from '../../src/client.js';
import { expectOk } from '../helpers/result.js';
import { mockProjects } from '../mocks/handlers.js';
import { server } from '../setup.js';

describe('RustrakClient', () => {
  // Without an explicit `Accept` every method would send `*/*`, so a
  // content-negotiating proxy could answer with HTML and the caller would see
  // `invalid_response` with no clue why.
  describe('Request headers', () => {
    async function headersOfNextRequest(
      client: RustrakClient,
    ): Promise<Headers> {
      let seen: Headers | undefined;
      server.use(
        http.get('http://localhost:8080/api/projects', ({ request }) => {
          seen = request.headers;
          return HttpResponse.json({
            items: [],
            total_count: 0,
            page: 1,
            per_page: 20,
            total_pages: 0,
          });
        }),
      );

      expectOk(await client.projects.list());
      if (seen === undefined) {
        expect.fail('the request never reached the handler');
      }
      return seen;
    }

    it('asks for JSON on every request', async () => {
      const headers = await headersOfNextRequest(
        new RustrakClient({ baseUrl: 'http://localhost:8080' }),
      );

      expect(headers.get('accept')).toBe('application/json');
    });

    it('lets a caller override Accept', async () => {
      const headers = await headersOfNextRequest(
        new RustrakClient({
          baseUrl: 'http://localhost:8080',
          headers: { Accept: 'application/vnd.rustrak+json' },
        }),
      );

      expect(headers.get('accept')).toBe('application/vnd.rustrak+json');
    });

    // A JSON write spreads the caller's headers into an object, which keeps
    // nothing of a `Headers` instance or an array of pairs.
    it.each([
      ['Headers', new Headers({ Authorization: 'Bearer per-call' })],
      ['pairs', [['Authorization', 'Bearer per-call']] as [string, string][]],
    ])('keeps per-call %s on a JSON write', async (_, headers) => {
      let seen: Headers | undefined;
      server.use(
        http.post('http://localhost:8080/api/projects', ({ request }) => {
          seen = request.headers;
          return new HttpResponse(null, { status: 500 });
        }),
      );

      await new RustrakClient({
        baseUrl: 'http://localhost:8080',
      }).projects.create({
        body: { name: 'Headers' },
        headers,
      });

      expect(seen?.get('authorization')).toBe('Bearer per-call');
    });
  });

  describe('Binary responses', () => {
    it('downloads an attachment as bytes', async () => {
      const png = new Uint8Array([
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a,
      ]);
      server.use(
        http.get(
          'http://localhost:8080/api/projects/1/attachments/:id',
          () =>
            new HttpResponse(png, { headers: { 'Content-Type': 'image/png' } }),
        ),
      );
      const client = new RustrakClient({
        baseUrl: 'http://localhost:8080',
        token: 'test-token',
      });

      const result = await client.events.downloadAttachment({
        path: {
          project_id: 1,
          attachment_id: '00000000-0000-4000-8000-000000000000',
        },
      });

      const blob = expectOk(result);
      expect(new Uint8Array(await blob.arrayBuffer())).toEqual(png);
    });
  });

  // Keys imported from Sentry are 32 random hex digits, so their version and
  // variant nibbles rarely form a valid RFC 4122 UUID. One such key must not
  // make the project list fail validation.
  it('lists a project whose key is hex but not an RFC 4122 UUID', async () => {
    const key = 'ffffffff-ffff-ffff-ffff-ffffffffffff';
    server.use(
      http.get('http://localhost:8080/api/projects', () =>
        HttpResponse.json({
          items: [{ ...mockProjects[0], sentry_key: key }],
          total_count: 1,
          page: 1,
          per_page: 20,
          total_pages: 1,
        }),
      ),
    );
    const client = new RustrakClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
    });

    const { items } = expectOk(await client.projects.list());

    expect(items.map((p) => p.sentry_key)).toEqual([key]);
  });
});
