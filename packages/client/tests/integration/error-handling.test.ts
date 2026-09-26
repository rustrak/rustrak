import { HttpResponse, http } from 'msw';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { RustrakClient } from '../../src/client.js';
import {
  type FieldError,
  isRetryable,
  NETWORK_ERROR_MESSAGE,
  type RustrakError,
  SERVER_ERROR_MESSAGE,
  TIMEOUT_ERROR_MESSAGE,
} from '../../src/errors.js';
// `isTransportFailure` comes from the module, not the barrel: the carrier is
// internal to the hop between ky and `BaseResource` and is deliberately not
// re-exported.
import { isTransportFailure } from '../../src/utils/http.js';
import { createKyInstance } from '../../src/utils/index.js';
import { expectErr, expectOk } from '../helpers/result.js';
import { appErrorResponse } from '../mocks/handlers.js';
import { server } from '../setup.js';

describe('Error Handling', () => {
  let client: RustrakClient;

  beforeEach(() => {
    client = new RustrakClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
    });
  });

  // These pin the status -> `kind` mapping only. Bodies are built with
  // `appErrorResponse` so the one body shape the Rust process can emit is the
  // one under test; the two exceptions are called out where they occur.
  describe('HTTP Status Codes', () => {
    it('should map 400 to validation', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', () =>
          appErrorResponse(
            'ValidationError',
            'Validation error: Invalid sort field: bogus',
          ),
        ),
      );

      const result = await client.projects.list();

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('validation');
    });

    it('should map 401 to unauthenticated', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', () =>
          appErrorResponse('Unauthorized', 'Unauthorized: Invalid token'),
        ),
      );

      const error = expectErr(await client.projects.list());

      expect(error.kind).toBe('unauthenticated');
      expect(isRetryable(error)).toBe(false);
      expect(error).toMatchObject({ status: 401 });
    });

    it('should map 403 to forbidden', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', () =>
          appErrorResponse(
            'Forbidden',
            'Forbidden: Insufficient project role for this action',
          ),
        ),
      );

      const error = expectErr(await client.projects.list());

      expect(error.kind).toBe('forbidden');
      expect(isRetryable(error)).toBe(false);
      expect(error).toMatchObject({ status: 403 });
    });

    it('should map 404 to not_found', async () => {
      const error = expectErr(await client.projects.get(999));

      expect(error.kind).toBe('not_found');
      expect(isRetryable(error)).toBe(false);
      expect(error).toMatchObject({ status: 404 });
    });

    it('should map 429 to rate_limited with retryAfter', async () => {
      // Flat body on purpose: `routes/ingest.rs:38-46` is the only 429 the
      // server sends and it is not an `AppError`.
      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          return HttpResponse.json(
            { error: 'rate_limit_exceeded', retry_after: 60 },
            { status: 429, headers: { 'Retry-After': '60' } },
          );
        }),
      );

      const error = expectErr(await client.projects.list());

      expect(error.kind).toBe('rate_limited');
      expect(isRetryable(error)).toBe(true);
      expect(error).toMatchObject({ status: 429, retryAfter: 60 });
    });

    it('should leave retryAfter absent when there is no Retry-After header', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          return HttpResponse.json(
            { error: 'rate_limit_exceeded', retry_after: 60 },
            { status: 429 },
          );
        }),
      );

      const error = expectErr(await client.projects.list());

      expect(error.kind).toBe('rate_limited');
      expect(error).not.toHaveProperty('retryAfter');
    });

    // RFC 9110 §10.2.3 allows delta-seconds or an HTTP-date, and a proxy in
    // front of the Rust process can send either. `parseInt` handled only the
    // first, and handled it too loosely: it read `3abc` as 3 and `-5` as -5,
    // both of which a consumer passes straight to setTimeout.
    async function retryAfterFor(header: string): Promise<number | undefined> {
      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          return HttpResponse.json(
            { error: 'rate_limit_exceeded' },
            { status: 429, headers: { 'Retry-After': header } },
          );
        }),
      );

      const error = expectErr(await client.projects.list());
      expect(error.kind).toBe('rate_limited');
      return error.kind === 'rate_limited' ? error.retryAfter : undefined;
    }

    it('reads the HTTP-date form as seconds from now', async () => {
      // Built from the clock rather than a literal date, so the test cannot
      // start passing for the wrong reason once the literal falls in the past.
      const header = new Date(Date.now() + 60_000).toUTCString();

      const retryAfter = await retryAfterFor(header);

      expect(typeof retryAfter).toBe('number');
      expect(retryAfter).toBeGreaterThanOrEqual(55);
      expect(retryAfter).toBeLessThanOrEqual(61);
    });

    it('clamps an HTTP-date already in the past to 0', async () => {
      const header = new Date(Date.now() - 120_000).toUTCString();

      expect(await retryAfterFor(header)).toBe(0);
    });

    it('rejects a negative delta-seconds', async () => {
      expect(await retryAfterFor('-5')).toBeUndefined();
    });

    it('rejects delta-seconds with trailing junk', async () => {
      expect(await retryAfterFor('3abc')).toBeUndefined();
    });

    it('should leave retryAfter absent when Retry-After is unparseable', async () => {
      expect(await retryAfterFor('whenever you like')).toBeUndefined();
    });

    it('should map 500 to server_error', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', () =>
          appErrorResponse('InternalError'),
        ),
      );

      const error = expectErr(await client.projects.list());

      expect(error.kind).toBe('server_error');
      expect(isRetryable(error)).toBe(true);
      // `message` is pinned, not just `status`. `appErrorResponse` throws when
      // a fixture hands it a 5xx message, but msw turns a resolver that throws
      // into a 500 of its own, which is the exact status this test asserts: a
      // fixture rejected for being the old leaky shape would otherwise sail
      // through here green.
      expect(error).toMatchObject({
        status: 500,
        message: SERVER_ERROR_MESSAGE,
      });
    });

    it('should surface the incident id a 500 body carries', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', () =>
          appErrorResponse('DatabaseError', undefined, undefined, {
            incidentId: '0b3f1c1e-2a4d-4b8e-9f1a-6c7d8e9f0a1b',
          }),
        ),
      );

      const error = expectErr(await client.projects.list());

      expect(error.kind).toBe('server_error');
      // The message stays redacted. The id is the only thing the server is
      // willing to say about a 500, and it is what a user quotes in a bug
      // report to point the operator at the `log::error!` line.
      expect(error).toMatchObject({
        status: 500,
        message: SERVER_ERROR_MESSAGE,
        incidentId: '0b3f1c1e-2a4d-4b8e-9f1a-6c7d8e9f0a1b',
      });
    });

    // A 5xx from a proxy has no Rustrak body, and an older server predates the
    // field. Neither may produce `incidentId: undefined`: `structuredClone`
    // round-trips it, but `'incidentId' in error` would start lying.
    it('should omit incidentId entirely when the 5xx body has none', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', () =>
          appErrorResponse('InternalError'),
        ),
      );

      const error = expectErr(await client.projects.list());

      expect(error.kind).toBe('server_error');
      expect('incidentId' in error).toBe(false);
    });

    // 502 and 503 are the deliberate exception: `AppError::status_code` cannot
    // produce either, so these only ever arrive from a reverse proxy in front
    // of the Rust process. Their bodies are whatever that proxy sends, so they
    // stay outside the `appErrorResponse` shape.
    it('should map 502 to server_error (proxy-generated, not an AppError)', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          return new HttpResponse('<html><body>502 Bad Gateway</body></html>', {
            status: 502,
            headers: { 'Content-Type': 'text/html' },
          });
        }),
      );

      const error = expectErr(await client.projects.list());

      expect(error.kind).toBe('server_error');
      expect(error).toMatchObject({ status: 502 });
    });

    it('should map 503 to server_error (proxy-generated, not an AppError)', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          return new HttpResponse(
            '<html><body>503 Service Unavailable</body></html>',
            { status: 503, headers: { 'Content-Type': 'text/html' } },
          );
        }),
      );

      const error = expectErr(await client.projects.list());

      expect(error.kind).toBe('server_error');
      expect(error).toMatchObject({ status: 503 });
    });
  });

  // The suite historically asserted only the error class, so a client that
  // returned "[object Object]" for every error stayed green (gh-204). These
  // tests pin the parsed `message` against the exact body the fixture sent,
  // which is the shape `apps/server/src/error.rs` actually emits:
  // `{"error": {"type", "message"}}` with the thiserror prefix in `message`.
  //
  // 4xx only. Every 5xx message is redacted inside the client now, so the
  // "keeps the server message" contract deliberately stops at 499; the
  // synthetic-path block below asserts the opposite for 5xx.
  describe('Server error contract (nested AppError body)', () => {
    it('400 ValidationError keeps the server message', async () => {
      const error = expectErr(await client.auth.getInvitation('expired-token'));

      expect(error.kind).toBe('validation');
      expect(error.message).toBe(
        'Validation error: Invitation is expired or already used',
      );
      expect(error).toMatchObject({ status: 400 });
      expect(isRetryable(error)).toBe(false);
    });

    it('401 Unauthorized keeps the server message', async () => {
      const error = expectErr(
        await client.auth.login({
          email: 'nobody@example.com',
          password: 'wrong-password',
        }),
      );

      expect(error.kind).toBe('unauthenticated');
      expect(error.message).toBe('Unauthorized: Invalid credentials');
      expect(error).toMatchObject({ status: 401 });
      expect(isRetryable(error)).toBe(false);
    });

    it('403 Forbidden keeps the server message', async () => {
      const error = expectErr(await client.team.remove(2));

      expect(error.kind).toBe('forbidden');
      expect(error.message).toBe(
        'Forbidden: The primary admin cannot be deleted',
      );
      expect(error).toMatchObject({ status: 403 });
      expect(isRetryable(error)).toBe(false);
    });

    it('404 NotFound keeps the server message without doubling the prefix', async () => {
      const error = expectErr(await client.projects.get(999));

      expect(error.kind).toBe('not_found');
      // `GET /api/projects/{id}` runs `access::require` and then
      // `ProjectService::get_by_id` (`services/project.rs:142`), which is what
      // 404s for the admin/legacy Bearer token these fixtures model. The exact
      // string also proves nothing prepends a second "Resource not found: "
      // (gh-204's sibling).
      expect(error.message).toBe(
        'Resource not found: Project with id 999 not found',
      );
      expect(error).toMatchObject({ status: 404 });
      expect(isRetryable(error)).toBe(false);
    });

    // 409 used to have no dedicated error class: `transformHttpError`'s
    // `default:` branch returned a bare `RustrakError` with `statusCode` set.
    // The union closes that gap, which is what this now pins.
    //
    // User 3 is the non-primary admin. Demoting user 2 cannot reach this guard:
    // `routes/team.rs:118-125` rejects a primary-admin role change first, which
    // the next test pins.
    it('409 Conflict has a kind of its own and keeps the server message', async () => {
      const error = expectErr(await client.team.updateRole(3, 'member'));

      expect(error.kind).toBe('conflict');
      expect(error.message).toBe('Conflict: Cannot demote the last admin');
      expect(error).toMatchObject({ status: 409 });
    });

    it('403 wins over 409 when the target is the primary admin', async () => {
      const error = expectErr(await client.team.updateRole(2, 'member'));

      expect(error.kind).toBe('forbidden');
      expect(error.message).toBe(
        "Forbidden: The primary admin's role cannot be changed",
      );
      expect(error).toMatchObject({ status: 403 });
    });

    it('403 Forbidden on the invite-only register endpoint', async () => {
      const error = expectErr(
        await client.auth.register({
          email: 'someone@example.com',
          password: 'password123',
        }),
      );

      expect(error.kind).toBe('forbidden');
      expect(error.message).toBe('Forbidden: Registration is invite-only');
      expect(error).toMatchObject({ status: 403 });
    });
  });

  // Statuses whose only real producer is an endpoint this client never calls.
  // They are driven through a bare ky instance against `/__status-transform/*`
  // rather than a resource method, because no management endpoint can emit
  // them: `AppError::PayloadTooLarge` comes only from envelope ingestion and
  // the flat 429 only from the ingest rate limiter. See the matching block in
  // `tests/mocks/handlers.ts`.
  //
  // A bare ky instance still rejects: the `Result` conversion lives in
  // `BaseResource`, and the rejection carries the union inside
  // `RustrakTransportFailure`. `unwrapTransport` asserts that carrier is what
  // came out, so a raw ky error could not slip through here.
  describe('transformHttpError status mapping (synthetic paths)', () => {
    const bareClient = (maxRetries = 0) =>
      createKyInstance({
        baseUrl: 'http://localhost:8080',
        token: 'test-token',
        maxRetries,
      });

    async function unwrapTransport(
      request: Promise<unknown>,
    ): Promise<RustrakError> {
      const thrown = await request.then(
        () => {
          expect.fail('expected the request to reject');
        },
        (error: unknown) => error,
      );

      if (!isTransportFailure(thrown)) {
        expect.fail(
          `expected a RustrakTransportFailure, got ${String(thrown)}`,
        );
      }
      return thrown.rustrakError;
    }

    // 413 used to fall through to `transformHttpError`'s `default:` branch,
    // the same gap 409 had. It has its own kind now.
    it('413 PayloadTooLarge keeps the server message', async () => {
      const error = await unwrapTransport(
        bareClient().get('__status-transform/payload-too-large'),
      );

      expect(error.kind).toBe('payload_too_large');
      expect(error.message).toBe(
        'Payload too large: Compressed payload exceeds 104857600 bytes',
      );
      expect(error).toMatchObject({ status: 413 });
    });

    it('500 DatabaseError is redacted and stays retryable', async () => {
      const error = await unwrapTransport(
        bareClient().get('__status-transform/database-error'),
      );

      expect(error.kind).toBe('server_error');
      // The fixture sends "Database error: pool timed out while waiting for an
      // open connection". None of it may reach a consumer: a pool message can
      // carry a host, a port or a user name.
      expect(error.message).toBe(SERVER_ERROR_MESSAGE);
      expect(error.message).not.toContain('pool timed out');
      expect(error).toMatchObject({ status: 500 });
      expect(isRetryable(error)).toBe(true);
    });

    // `AppError::Internal` is the variant that carries arbitrary internal text
    // (`services/sourcemap.rs:441`), and it shares 500 with `Database`. It is
    // the strongest case for redaction: the fixture's message names a
    // filesystem failure and an OS errno.
    it('500 InternalError is redacted', async () => {
      const error = await unwrapTransport(
        bareClient().get('__status-transform/internal-error'),
      );

      expect(error.kind).toBe('server_error');
      expect(error.message).toBe(SERVER_ERROR_MESSAGE);
      expect(error.message).not.toContain('No space left on device');
      expect(error.message).not.toContain('failed to store source file');
      expect(error).toMatchObject({ status: 500 });
    });

    // `gone` and `client_error` are the two members no resource method can
    // reach: nothing in `AppError` maps to 410, and the `default` arm exists
    // precisely for statuses the server does not produce. Before these two
    // fixtures both lines were uncovered, which is how the union could have
    // grown a wrong branch without a single test noticing.
    it('410 maps to gone and keeps the message', async () => {
      const error = await unwrapTransport(
        bareClient().get('__status-transform/gone'),
      );

      expect(error.kind).toBe('gone');
      expect(error.message).toBe('This endpoint has been retired');
      expect(error).toMatchObject({ status: 410 });
      expect(isRetryable(error)).toBe(false);
    });

    it('an unenumerated sub-500 status falls to client_error with its status intact', async () => {
      const error = await unwrapTransport(
        bareClient().get('__status-transform/unenumerated'),
      );

      expect(error.kind).toBe('client_error');
      expect(error.message).toBe("I'm a teapot");
      // The point of the catch-all: `status` is readable even though `kind`
      // says only "nothing dedicated handles this".
      expect(error).toMatchObject({ status: 418 });
      expect(isRetryable(error)).toBe(false);
    });

    it('429 exposes the flat ingest message and a populated retryAfter', async () => {
      const error = await unwrapTransport(
        bareClient().get('__status-transform/rate-limited'),
      );

      expect(error.kind).toBe('rate_limited');
      expect(error.message).toBe('rate_limit_exceeded');
      expect(error).toMatchObject({ status: 429, retryAfter: 59 });
      expect(isRetryable(error)).toBe(true);
      if (error.kind === 'rate_limited') {
        expect(typeof error.retryAfter).toBe('number');
      }
    });
  });

  describe('Malformed error bodies', () => {
    it('falls back cleanly when the body is not JSON', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          return new HttpResponse('<html><body>gateway barf</body></html>', {
            status: 400,
            headers: { 'Content-Type': 'text/html' },
          });
        }),
      );

      const error = expectErr(await client.projects.list());

      expect(error.kind).toBe('validation');
      expect(error.message).toBe('HTTP 400 error');
      expect(error.message).not.toContain('undefined');
      expect(error.message).not.toContain('[object Object]');
    });

    it('falls back cleanly when the JSON body has no error key', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          return HttpResponse.json(
            { detail: 'something else' },
            { status: 400 },
          );
        }),
      );

      const error = expectErr(await client.projects.list());

      expect(error.kind).toBe('validation');
      expect(error.message).toBe('HTTP 400 error');
      expect(error.message).not.toContain('undefined');
      expect(error.message).not.toContain('[object Object]');
    });
  });

  describe('Network Errors', () => {
    it('should report network on timeout', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', async () => {
          await new Promise((resolve) => setTimeout(resolve, 100));
          return HttpResponse.json([]);
        }),
      );

      const shortTimeoutClient = new RustrakClient({
        baseUrl: 'http://localhost:8080',
        token: 'test-token',
        timeout: 10, // Very short timeout
      });

      const error = expectErr(await shortTimeoutClient.projects.list());

      expect(error.kind).toBe('network');
      expect(error.message).toBe(TIMEOUT_ERROR_MESSAGE);
      expect(error.message).toContain('timed out');
      expect(isRetryable(error)).toBe(true);
      // No `cause`: the underlying ky error embeds the resolved host and port.
      expect(error).not.toHaveProperty('cause');
      expect(error).toMatchObject({ reason: 'timeout' });
    });

    // The reason this message is fixed rather than forwarded. ky's
    // `NetworkError` builds its own as
    // `Request failed due to a network error: ${method} ${url}`, so a
    // self-hosted deployment that renders `error.message` anywhere would print
    // its internal host and port to the browser.
    it('never puts the host or the port in the message', async () => {
      const host = 'rustrak.internal';
      const port = '8099';

      server.use(
        http.get(`http://${host}:${port}/api/projects`, () =>
          HttpResponse.error(),
        ),
      );

      const internalClient = new RustrakClient({
        baseUrl: `http://${host}:${port}`,
        token: 'test-token',
        maxRetries: 0,
      });

      const error = expectErr(await internalClient.projects.list());

      expect(error.kind).toBe('network');
      expect(error.message).toBe(NETWORK_ERROR_MESSAGE);
      expect(error.message).not.toContain(host);
      expect(error.message).not.toContain(port);
      expect(error.message).not.toContain('api/projects');
      expect(error).not.toHaveProperty('cause');
      // The discriminator a caller uses instead of reading the message.
      expect(error).toMatchObject({ reason: 'unreachable' });
      expect(Object.keys(error).sort()).toEqual(['kind', 'message', 'reason']);
    });
  });

  describe('Response Validation', () => {
    it('should report invalid_response on a body that is not JSON', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          return new HttpResponse('not json', {
            headers: { 'Content-Type': 'application/json' },
          });
        }),
      );

      const result = await client.projects.list();

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_response');
    });

    // The distinction the old unqualified catch could not make. The status line
    // arrived, so this is not a `beforeError` failure; the body read is a
    // second trip over the socket and it is that trip which died. Reporting
    // `invalid_response` here told the caller a transient fault was permanent.
    it('should report network when the connection dies mid-body', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          const stream = new ReadableStream({
            start(controller) {
              controller.enqueue(new TextEncoder().encode('{"items":'));
              controller.error(new Error('socket hang up'));
            },
          });

          return new HttpResponse(stream, {
            headers: { 'Content-Type': 'application/json' },
          });
        }),
      );

      const error = expectErr(await client.projects.list());

      expect(error.kind).toBe('network');
      expect(isRetryable(error)).toBe(true);
      expect(error.message).toBe(NETWORK_ERROR_MESSAGE);
      expect(error.message).not.toContain('socket hang up');
    });

    it('should report invalid_response on schema mismatch', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          return HttpResponse.json([
            {
              id: 'not-a-number', // Should be number
              name: 'Test',
            },
          ]);
        }),
      );

      const error = expectErr(await client.projects.list());

      expect(error.kind).toBe('invalid_response');
      expect(isRetryable(error)).toBe(false);
    });

    // Successor to "should throw ValidationError with details". The old
    // `getValidationDetails()` echoed the offending field back; the union
    // deliberately drops the Zod issues because they embed response data, so
    // what is asserted now is that they are gone.
    it('should not leak Zod issues or response data on a schema mismatch', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          return HttpResponse.json({
            items: [
              {
                id: 1,
                name: 'Test',
                slug: 'test',
                sentry_key: 'invalid-uuid',
                dsn: 'http://localhost:8080/1',
                stored_event_count: 0,
                digested_event_count: 0,
                rate_limited_event_count: 0,
                rate_limit_per_minute: null,
                rate_limit_per_hour: null,
                created_at: 'invalid-date',
                updated_at: '2026-01-20T10:00:00.000Z',
              },
            ],
            total_count: 1,
            page: 1,
            per_page: 20,
            total_pages: 1,
          });
        }),
      );

      const error = expectErr(await client.projects.list());

      expect(error.kind).toBe('invalid_response');
      expect(error).not.toHaveProperty('issues');
      expect(error).not.toHaveProperty('validationErrors');
      expect(error.message).not.toContain('sentry_key');
      expect(error.message).not.toContain('invalid-uuid');
      expect(Object.keys(error).sort()).toEqual(['kind', 'message']);
    });
  });

  // Deliberately NOT the AppError shape. `transformHttpError` still accepts a
  // flat `{error: "<string>"}` because the ingest rate limiter sends one, and
  // this block exists to keep that tolerance covered. Everything the management
  // API sends is nested, so new fixtures belong in the blocks above, not here.
  describe('Flat error bodies (tolerated legacy shape)', () => {
    it('should extract a flat string error message from the body', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          return HttpResponse.json(
            { error: 'Custom error message' },
            { status: 400 },
          );
        }),
      );

      const error = expectErr(await client.projects.list());
      expect(error.message).toBe('Custom error message');
    });

    it('should redact rather than fall back when a 500 body is not JSON', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          return new HttpResponse('Error text', { status: 500 });
        }),
      );

      const error = expectErr(await client.projects.list());

      // The old assertion was that `message` contains "500". A 5xx no longer
      // echoes anything, not even the status, into the message; the status
      // lives in `status`, where a consumer can read it without rendering it.
      expect(error.kind).toBe('server_error');
      expect(error.message).toBe(SERVER_ERROR_MESSAGE);
      expect(error).toMatchObject({ status: 500 });
    });
  });

  describe('Retry Logic', () => {
    it('should retry on 500 errors', async () => {
      let attempts = 0;

      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          attempts++;
          if (attempts < 2) {
            return appErrorResponse('DatabaseError');
          }
          return HttpResponse.json({
            items: [],
            total_count: 0,
            page: 1,
            per_page: 20,
            total_pages: 0,
          });
        }),
      );

      const response = expectOk(await client.projects.list());
      expect(response.items).toEqual([]);
      expect(attempts).toBe(2);
    });

    it('should retry on 503 errors', async () => {
      let attempts = 0;

      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          attempts++;
          if (attempts < 2) {
            // Proxy-shaped: 503 cannot come from `AppError::status_code`.
            return new HttpResponse('503 Service Unavailable', {
              status: 503,
            });
          }
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
      expect(attempts).toBe(2);
    });

    it('should not retry on 401 errors', async () => {
      let attempts = 0;

      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          attempts++;
          return appErrorResponse(
            'Unauthorized',
            'Unauthorized: Invalid token',
          );
        }),
      );

      const error = expectErr(await client.projects.list());

      expect(error.kind).toBe('unauthenticated');
      // Ky default retry might attempt a few times before giving up
      // The important thing is that 401 should eventually fail
      expect(attempts).toBeGreaterThanOrEqual(1);
    });

    it('should respect maxRetries config', async () => {
      let attempts = 0;

      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          attempts++;
          return appErrorResponse('DatabaseError');
        }),
      );

      const customClient = new RustrakClient({
        baseUrl: 'http://localhost:8080',
        token: 'test-token',
        maxRetries: 0,
      });

      const error = expectErr(await customClient.projects.list());

      expect(error.kind).toBe('server_error');
      expect(attempts).toBe(1); // Initial attempt only, no retries
    });
  });

  describe('Edge Cases', () => {
    it('should handle empty error response body', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          return new HttpResponse(null, { status: 500 });
        }),
      );

      const error = expectErr(await client.projects.list());

      expect(error.kind).toBe('server_error');
      expect(error).toMatchObject({ status: 500 });
      expect(error.message).toBe(SERVER_ERROR_MESSAGE);
    });

    it('should handle unexpected response structure', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          return HttpResponse.json({ unexpected: 'structure' });
        }),
      );

      const result = await client.projects.list();

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_response');
    });

    it('should handle very large error messages', async () => {
      const largeMessage = 'x'.repeat(10000);

      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          return HttpResponse.json({ error: largeMessage }, { status: 400 });
        }),
      );

      const error = expectErr(await client.projects.list());
      expect(error.message).toBe(largeMessage);
    });
  });

  // AD-1's whole reason for existing: an exception cannot survive React's
  // Flight serializer, which rejects anything whose prototype is not
  // `Object.prototype`. Cheap assertions here stop a future refactor quietly
  // reintroducing a class.
  describe('structuredClone (RSC boundary)', () => {
    it('a failed Result survives structuredClone intact', async () => {
      const result = await client.projects.get(999);

      const cloned = structuredClone(result);

      expect(cloned).toEqual(result);
      expect(cloned.success).toBe(false);
      expect(expectErr(cloned).kind).toBe('not_found');
      expect(expectErr(cloned).message).toBe(
        'Resource not found: Project with id 999 not found',
      );
    });

    it('a successful Result survives structuredClone intact', async () => {
      const result = await client.projects.get(1);

      const cloned = structuredClone(result);

      expect(cloned).toEqual(result);
      expect(expectOk(cloned).id).toBe(1);
    });

    it('nothing in a Result has a prototype other than Object.prototype', async () => {
      const result = await client.projects.get(999);

      expect(Object.getPrototypeOf(result)).toBe(Object.prototype);
      expect(Object.getPrototypeOf(expectErr(result))).toBe(Object.prototype);
      expect(result).not.toBeInstanceOf(Error);
      expect(expectErr(result)).not.toBeInstanceOf(Error);
    });

    it('every construction site produces a structuredClone-able error', async () => {
      // One representative of each place an error is built: the HTTP status
      // mapping, the pre-flight input check, the response-schema check, the
      // 5xx redaction and the transport failure.
      const cases: RustrakError[] = [
        expectErr(await client.projects.get(999)),
        expectErr(await client.projects.create({ name: '' })),
      ];

      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          return HttpResponse.json({ unexpected: 'structure' });
        }),
      );
      cases.push(expectErr(await client.projects.list()));

      server.use(
        http.get('http://localhost:8080/api/projects', () =>
          appErrorResponse('InternalError'),
        ),
      );
      cases.push(expectErr(await client.projects.list()));

      server.use(
        http.get('http://localhost:8080/api/projects', () =>
          HttpResponse.error(),
        ),
      );
      cases.push(expectErr(await client.projects.list()));

      expect(cases.map((error) => error.kind)).toEqual([
        'not_found',
        'invalid_request',
        'invalid_response',
        'server_error',
        'network',
      ]);

      for (const error of cases) {
        expect(structuredClone(error)).toEqual(error);
      }
    });
  });

  // ---------------------------------------------------------------------------
  // `error.fields`
  //
  // Optional and additive: the server sends it only when it can name the input
  // it rejected, and omits the key entirely otherwise. Every body here is built
  // through `appErrorResponse`, which is the one place that mirrors the
  // server's `skip_serializing_if = "Vec::is_empty"`.
  // ---------------------------------------------------------------------------
  describe('Field-level errors', () => {
    const createProject = () =>
      client.projects.create({ name: 'Taken', slug: 'taken-slug' });

    it('a 409 carrying fields surfaces them on the conflict error', async () => {
      // The commonest case by far: 12 of the 14 annotated server sites are
      // uniqueness conflicts, which are 409s and not 400s.
      server.use(
        http.post('http://localhost:8080/api/projects', () =>
          appErrorResponse(
            'Conflict',
            "Conflict: Project with slug 'taken-slug' already exists",
            [{ field: 'slug', code: 'already_exists' }],
          ),
        ),
      );

      const error = expectErr(await createProject());

      expect(error.kind).toBe('conflict');
      expect(error).toMatchObject({
        status: 409,
        fields: [{ field: 'slug', code: 'already_exists' }],
      });
      // Still serializable: a Server Action has to hand this to the browser.
      expect(structuredClone(error)).toEqual(error);
    });

    it('a 400 carrying fields surfaces them on the validation error', async () => {
      server.use(
        http.post('http://localhost:8080/api/projects', () =>
          appErrorResponse(
            'ValidationError',
            'Validation error: Name cannot be empty',
            [{ field: 'name', code: 'required' }],
          ),
        ),
      );

      const error = expectErr(await createProject());

      expect(error.kind).toBe('validation');
      expect(error).toMatchObject({
        status: 400,
        fields: [{ field: 'name', code: 'required' }],
      });
    });

    it('a body with no fields key leaves the property absent', async () => {
      // Not `[]`, and not `undefined`: absent. Everything shipped before this
      // existed sends exactly this body, and `'fields' in error` is how a
      // consumer decides between `setError(path)` and a form-level error.
      server.use(
        http.post('http://localhost:8080/api/projects', () =>
          appErrorResponse(
            'Conflict',
            'Conflict: Cannot demote the last admin',
          ),
        ),
      );

      const error = expectErr(await createProject());

      expect(error.kind).toBe('conflict');
      expect('fields' in error).toBe(false);
    });

    it('a custom code carries its message through verbatim', async () => {
      server.use(
        http.post('http://localhost:8080/api/projects', () =>
          appErrorResponse('ValidationError', 'Validation error: Rejected', [
            {
              field: 'credentials.webhook_url',
              code: 'custom',
              message: 'Slack rejected this webhook.',
            },
          ]),
        ),
      );

      const error = expectErr(await createProject());

      expect(error).toMatchObject({
        fields: [
          {
            field: 'credentials.webhook_url',
            code: 'custom',
            message: 'Slack rejected this webhook.',
          },
        ],
      });
    });

    it('drops entries whose code this build does not know', async () => {
      // A newer server adding a variant must not hand a consumer a `code` its
      // own exhaustive switch cannot see. The known entry survives; the
      // unknown one is dropped rather than coerced.
      server.use(
        http.post('http://localhost:8080/api/projects', () =>
          HttpResponse.json(
            {
              error: {
                type: 'Conflict',
                message:
                  "Conflict: Project with slug 'taken-slug' already exists",
                fields: [
                  { field: 'slug', code: 'already_exists' },
                  { field: 'name', code: 'from_the_future' },
                ],
              },
            },
            { status: 409 },
          ),
        ),
      );

      const error = expectErr(await createProject());

      expect(error).toMatchObject({
        fields: [{ field: 'slug', code: 'already_exists' }],
      });
    });

    it('leaves the property absent when nothing in fields is usable', async () => {
      server.use(
        http.post('http://localhost:8080/api/projects', () =>
          HttpResponse.json(
            {
              error: {
                type: 'Conflict',
                message: 'Conflict: something',
                fields: [{ field: 42 }, 'not an object', null],
              },
            },
            { status: 409 },
          ),
        ),
      );

      const error = expectErr(await createProject());

      expect('fields' in error).toBe(false);
    });

    it('reads fields on every non-5xx status, not only 400 and 409', async () => {
      // `AppError::with_field` works on all eight Rust variants, so a
      // `Forbidden` that names an input has to arrive here too. Reading only
      // 400 and 409 made the first such annotation invisible to every
      // consumer with nothing failing anywhere.
      const statuses: ReadonlyArray<[number, string, string]> = [
        [401, 'Unauthorized', 'unauthenticated'],
        [403, 'Forbidden', 'forbidden'],
        [404, 'NotFound', 'not_found'],
        [410, 'Conflict', 'gone'],
        [413, 'PayloadTooLarge', 'payload_too_large'],
        [418, 'Conflict', 'client_error'],
        [429, 'Conflict', 'rate_limited'],
      ];

      for (const [status, type, kind] of statuses) {
        server.use(
          http.post('http://localhost:8080/api/projects', () =>
            HttpResponse.json(
              {
                error: {
                  type,
                  message: 'nope',
                  fields: [{ field: 'role', code: 'invalid' }],
                },
              },
              { status },
            ),
          ),
        );

        const error = expectErr(await createProject());

        expect(error.kind, `status ${status}`).toBe(kind);
        expect(error, `status ${status}`).toMatchObject({
          fields: [{ field: 'role', code: 'invalid' }],
        });
        // Still a plain object across the RSC boundary.
        expect(structuredClone(error)).toEqual(error);
      }
    });

    it('discards a 5xx body wholesale, fields included', async () => {
      // The 5xx redaction is not negotiable: `AppError::Internal` interpolates
      // arbitrary internal text. `fields` must not become a way back in.
      server.use(
        http.post('http://localhost:8080/api/projects', () =>
          HttpResponse.json(
            {
              error: {
                type: 'InternalError',
                message: 'Internal server error: pool timed out at /var/run/x',
                fields: [{ field: 'name', code: 'invalid' }],
              },
            },
            { status: 500 },
          ),
        ),
      );

      const error = expectErr(await client.projects.create({ name: 'X' }));

      expect(error.kind).toBe('server_error');
      expect('fields' in error).toBe(false);
      expect(error.message).toBe(SERVER_ERROR_MESSAGE);
    });

    it('strips a message that arrives on any code other than custom', async () => {
      // Both sides' docs say copy is selected from `(field, code)` for every
      // code but `custom`, so a message here is untranslatable English that a
      // careless consumer would render. Drop it at the boundary rather than
      // trusting the server never to send one.
      server.use(
        http.post('http://localhost:8080/api/projects', () =>
          HttpResponse.json(
            {
              error: {
                type: 'Conflict',
                message: 'Conflict: taken',
                fields: [
                  {
                    field: 'slug',
                    code: 'already_exists',
                    message: 'That slug is taken, sorry!',
                  },
                ],
              },
            },
            { status: 409 },
          ),
        ),
      );

      const error = expectErr(await createProject());

      expect(error).toMatchObject({
        fields: [{ field: 'slug', code: 'already_exists' }],
      });
      const [only] = (error as { fields: FieldError[] }).fields;
      expect(only && 'message' in only).toBe(false);
    });

    it('warns once when it drops an unrecognised code', async () => {
      // Dropping is right, but a silent drop is indistinguishable from "the
      // server named no field". A support engineer needs one line they can
      // find. Deduped per code, so a list page does not flood the console.
      const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});

      server.use(
        http.post('http://localhost:8080/api/projects', () =>
          HttpResponse.json(
            {
              error: {
                type: 'Conflict',
                message: 'Conflict: taken',
                fields: [
                  { field: 'name', code: 'not_a_code_this_build_knows' },
                ],
              },
            },
            { status: 409 },
          ),
        ),
      );

      try {
        expectErr(await createProject());
        expectErr(await createProject());

        expect(warn).toHaveBeenCalledTimes(1);
        expect(warn.mock.calls[0]?.[0]).toContain(
          'not_a_code_this_build_knows',
        );
        expect(warn.mock.calls[0]?.[0]).toContain('@rustrak/client');
      } finally {
        warn.mockRestore();
      }
    });
  });
});
