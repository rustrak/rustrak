import { HttpResponse, http } from 'msw/http';
import type { FieldError } from '../../src/errors.js';

const BASE_URL = 'http://localhost:8080';

/**
 * The eight `type` literals `AppError::error_response` can emit
 * (`apps/server/src/error.rs`). Declaring them as a union means a typo in a
 * fixture is a compile error rather than a body that silently stops matching
 * the server.
 */
export type AppErrorType =
  | 'NotFound'
  | 'ValidationError'
  | 'Conflict'
  | 'Unauthorized'
  | 'Forbidden'
  | 'PayloadTooLarge'
  | 'DatabaseError'
  | 'InternalError';

/**
 * The `Display` prefix `thiserror` puts in front of each variant's detail, from
 * the `#[error("...")]` attributes in `apps/server/src/error.rs`.
 *
 * `tests/unit/app-error-contract.test.ts` reads that Rust file and asserts this
 * table still matches it, so a reworded `#[error(...)]` fails the TypeScript
 * suite instead of drifting past it.
 */
export const APP_ERROR_PREFIXES = {
  NotFound: 'Resource not found: ',
  ValidationError: 'Validation error: ',
  Conflict: 'Conflict: ',
  Unauthorized: 'Unauthorized: ',
  Forbidden: 'Forbidden: ',
  PayloadTooLarge: 'Payload too large: ',
  DatabaseError: 'Database error: ',
  InternalError: 'Internal server error: ',
} as const satisfies Record<AppErrorType, string>;

/**
 * `AppError::status_code` (`apps/server/src/error.rs`) is a total function of
 * the variant, so the status is derived here rather than passed in: the server
 * cannot produce a `NotFound` that is not a 404, and neither can a fixture.
 * The same contract test above checks this table against the Rust `match`.
 */
export const APP_ERROR_STATUS = {
  NotFound: 404,
  ValidationError: 400,
  Conflict: 409,
  Unauthorized: 401,
  Forbidden: 403,
  PayloadTooLarge: 413,
  DatabaseError: 500,
  InternalError: 500,
} as const satisfies Record<AppErrorType, number>;

/**
 * The fixed `message` every 5xx body carries, mirroring
 * `INTERNAL_ERROR_MESSAGE` in `apps/server/src/error.rs`.
 *
 * `tests/unit/app-error-contract.test.ts` reads that constant out of the Rust
 * file and asserts it still equals this one, so rewording it on either side is
 * a failing test rather than a fixture that quietly stops matching the server.
 */
export const INTERNAL_ERROR_MESSAGE = 'Internal server error';

/**
 * Build the nested body every `AppError` produces:
 * `{"error": {"type": ..., "message": ...}}`, with the status derived from the
 * variant.
 *
 * **4xx**: `message` is `AppError`'s `Display`, so it always carries the
 * thiserror prefix: a 404 reads `Resource not found: <detail>`, never a bare
 * detail. Passing a message without its prefix throws here rather than
 * producing a body the server could never send.
 *
 * **5xx**: there is no caller-supplied message at all. `error_response`
 * replaces `Display` with {@link INTERNAL_ERROR_MESSAGE} so a `sqlx::Error`'s
 * constraint and column names never reach the wire (gh-233), which means a
 * fixture that passed its own 500 message would be testing a body the server
 * stopped emitting. Passing one throws. `incidentId` is what a 500 carries
 * instead, and it is optional here because a proxy-generated 5xx and a server
 * older than gh-233 both send a body without one.
 *
 * `fields` mirrors the server exactly: `ErrorDetail.fields` is
 * `#[serde(skip_serializing_if = "Vec::is_empty")]`, so an absent or empty list
 * means the key is absent from the body, never `[]` and never `null`. Fixtures
 * go through here rather than inlining a body precisely so that this stays true
 * in one place. `incident_id` is skipped the same way.
 */
export function appErrorResponse(
  type: AppErrorType,
  message?: string,
  fields?: FieldError[],
  options?: { incidentId?: string },
) {
  const status = APP_ERROR_STATUS[type];
  const isServerError = status >= 500;

  if (isServerError && message !== undefined) {
    throw new Error(
      `appErrorResponse('${type}', ...) takes no message: every 5xx body is ` +
        `${JSON.stringify(INTERNAL_ERROR_MESSAGE)}, fixed by the server.`,
    );
  }
  if (!isServerError && message === undefined) {
    throw new Error(`appErrorResponse('${type}', ...) requires a message.`);
  }

  const prefix = APP_ERROR_PREFIXES[type];
  if (message !== undefined && !message.startsWith(prefix)) {
    throw new Error(
      `appErrorResponse('${type}', ...) expects the server's Display prefix: ` +
        `message must start with ${JSON.stringify(prefix)}, got ${JSON.stringify(message)}`,
    );
  }

  return HttpResponse.json(
    {
      error: {
        type,
        message: isServerError ? INTERNAL_ERROR_MESSAGE : message,
        ...(fields !== undefined && fields.length > 0 ? { fields } : {}),
        ...(options?.incidentId !== undefined
          ? { incident_id: options.incidentId }
          : {}),
      },
    },
    { status },
  );
}

/** `ProjectService::get_by_id`'s 404, what an admin's Bearer token gets. */
export function projectNotFound(id: string | number | readonly string[]) {
  return `Resource not found: Project with id ${id} not found`;
}

// Mock data
export const mockProjects = [
  {
    id: 1,
    name: 'Test Project',
    slug: 'test-project',
    sentry_key: '123e4567-e89b-12d3-a456-426614174000',
    dsn: 'http://123e4567-e89b-12d3-a456-426614174000@localhost:8080/1',
    stored_event_count: 100,
    disabled_products: [],
    product_order: [],
    digested_event_count: 95,
    rate_limited_event_count: 0,
    rate_limit_per_minute: null,
    rate_limit_per_hour: null,
    created_at: '2026-01-20T10:00:00.000Z',
    updated_at: '2026-01-20T10:00:00.000Z',
    platform: 'javascript',
  },
  {
    id: 2,
    name: 'Another Project',
    slug: 'another-project',
    sentry_key: '223e4567-e89b-12d3-a456-426614174000',
    dsn: 'http://223e4567-e89b-12d3-a456-426614174000@localhost:8080/2',
    stored_event_count: 50,
    disabled_products: [],
    product_order: [],
    digested_event_count: 48,
    rate_limited_event_count: 0,
    rate_limit_per_minute: null,
    rate_limit_per_hour: null,
    created_at: '2026-01-19T10:00:00.000Z',
    updated_at: '2026-01-19T10:00:00.000Z',
    platform: null,
  },
];

export const handlers = [
  // Projects
  http.get(`${BASE_URL}/api/projects`, () => {
    return HttpResponse.json({
      items: mockProjects,
      total_count: mockProjects.length,
      page: 1,
      per_page: 20,
      total_pages: 1,
    });
  }),

  http.get(`${BASE_URL}/api/projects/:id`, ({ params }) => {
    const { id } = params;
    const project = mockProjects.find((p) => p.id === Number(id));

    if (!project) {
      return appErrorResponse('NotFound', projectNotFound(id));
    }

    return HttpResponse.json(project);
  }),

  http.post(`${BASE_URL}/api/projects`, async ({ request }) => {
    const body = (await request.json()) as {
      name: string;
      slug?: string;
      platform?: string;
    };

    const newProject = {
      id: 3,
      name: body.name,
      slug: body.slug ?? body.name.toLowerCase().replace(/\s+/g, '-'),
      sentry_key: '923e4567-e89b-12d3-a456-426614174000',
      dsn: 'http://923e4567-e89b-12d3-a456-426614174000@localhost:8080/3',
      stored_event_count: 0,
      disabled_products: [],
      product_order: [],
      digested_event_count: 0,
      rate_limited_event_count: 0,
      rate_limit_per_minute: null,
      rate_limit_per_hour: null,
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
      // Mirrors the server: an omitted platform stays NULL, a supplied one is
      // persisted and echoed back.
      platform: body.platform ?? null,
    };

    return HttpResponse.json(newProject, { status: 201 });
  }),
  // Only the failure: no test logs in successfully.
  http.post(`${BASE_URL}/auth/login`, () =>
    appErrorResponse('Unauthorized', 'Unauthorized: Invalid credentials'),
  ),

  // Public invitation lookup (accept page)
  http.get(`${BASE_URL}/auth/invitation/:token`, ({ params }) =>
    params.token === 'expired-token'
      ? appErrorResponse(
          'ValidationError',
          'Validation error: Invitation is expired or already used',
        )
      : appErrorResponse(
          'NotFound',
          'Resource not found: Invitation not found',
        ),
  ),

  http.patch(`${BASE_URL}/api/team/:userId/role`, () =>
    appErrorResponse('Conflict', 'Conflict: Cannot demote the last admin'),
  ),
  http.delete(`${BASE_URL}/api/team/:userId`, () =>
    appErrorResponse(
      'Forbidden',
      'Forbidden: The primary admin cannot be deleted',
    ),
  ),
];
