import { HttpResponse, http } from 'msw';
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

/**
 * The one flat error body the server still emits: the ingest rate limiter in
 * `apps/server/src/routes/ingest.rs`, which is not an `AppError` and so keeps
 * its own shape plus the `Retry-After` header.
 */
export function rateLimitResponse(retryAfter: number) {
  return HttpResponse.json(
    { error: 'rate_limit_exceeded', retry_after: retryAfter },
    { status: 429, headers: { 'Retry-After': String(retryAfter) } },
  );
}

/**
 * A missing project has two distinct wordings on the server and the actor
 * decides which one you get:
 *
 * - `ProjectService::get_by_id` (`services/project.rs:142`) says
 *   `Project with id {id} not found`. This is what an instance admin sees, and
 *   a Bearer token is either user-less (legacy, treated as admin) or owned by
 *   an admin in every fixture here, so `access::require` waves it through and
 *   the lookup is what 404s.
 * - `access::require` (`services/access.rs:65`) says `Project {id} not found`
 *   for a non-member, deliberately not leaking existence. Use
 *   `projectNotVisible` for endpoints whose only 404 comes from that guard.
 *
 * `services/project.rs:165` also has a bare `Project not found`, but it lives in
 * `get_by_sentry_key`, which is `#[allow(dead_code)]` and wired to no route, so
 * no client can ever observe it.
 */
export function projectNotFound(id: string | number | readonly string[]) {
  return `Resource not found: Project with id ${id} not found`;
}

/** `access::require`'s non-member 404 (`services/access.rs:65`). */
export function projectNotVisible(id: string | number | readonly string[]) {
  return `Resource not found: Project ${id} not found`;
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
    digested_event_count: 95,
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
    digested_event_count: 48,
    created_at: '2026-01-19T10:00:00.000Z',
    updated_at: '2026-01-19T10:00:00.000Z',
    platform: null,
  },
];

export const mockIssues = [
  {
    id: '323e4567-e89b-12d3-a456-426614174000',
    project_id: 1,
    short_id: 'TEST-1',
    title: 'TypeError: Cannot read property',
    value: "Cannot read property 'x' of undefined",
    culprit: 'handleRequest',
    logger: '',
    first_seen: '2026-01-20T10:00:00.000Z',
    last_seen: '2026-01-20T11:00:00.000Z',
    event_count: 5,
    level: 'error',
    platform: 'javascript',
    status: 'unresolved',
    substatus: 'new',
    priority: 'high',
    assigned_to: null,
    assignee_type: null,
    issue_type: 'error',
    issue_category: 'error',
    first_release: '',
    last_release: '',
    status_details: {},
    user_report_count: 0,
    is_resolved: false,
    is_muted: false,
  },
  {
    id: '423e4567-e89b-12d3-a456-426614174000',
    project_id: 1,
    short_id: 'TEST-2',
    title: 'ReferenceError: foo is not defined',
    value: 'foo is not defined',
    culprit: '',
    logger: '',
    first_seen: '2026-01-20T09:00:00.000Z',
    last_seen: '2026-01-20T10:00:00.000Z',
    event_count: 3,
    level: 'error',
    platform: 'javascript',
    status: 'unresolved',
    substatus: 'new',
    priority: 'high',
    assigned_to: null,
    assignee_type: null,
    issue_type: 'error',
    issue_category: 'error',
    first_release: '',
    last_release: '',
    status_details: {},
    user_report_count: 0,
    is_resolved: false,
    is_muted: false,
  },
];

export const mockEvents = [
  {
    id: '523e4567-e89b-12d3-a456-426614174000',
    event_id: '623e4567-e89b-12d3-a456-426614174000',
    issue_id: '323e4567-e89b-12d3-a456-426614174000',
    title: 'TypeError: Cannot read property',
    timestamp: '2026-01-20T11:00:00.000Z',
    level: 'error',
    platform: 'javascript',
    release: '1.0.0',
    environment: 'production',
    event_type: 'error',
  },
];

export const mockEventDetail = {
  id: '523e4567-e89b-12d3-a456-426614174000',
  event_id: '623e4567-e89b-12d3-a456-426614174000',
  issue_id: '323e4567-e89b-12d3-a456-426614174000',
  title: 'TypeError: Cannot read property',
  timestamp: '2026-01-20T11:00:00.000Z',
  ingested_at: '2026-01-20T11:00:01.000Z',
  level: 'error',
  platform: 'javascript',
  release: '1.0.0',
  environment: 'production',
  server_name: 'web-1',
  sdk_name: '@sentry/browser',
  sdk_version: '7.0.0',
  event_type: 'error',
  data: {
    exception: {
      values: [
        {
          type: 'TypeError',
          value: 'Cannot read property',
        },
      ],
    },
  },
};

export const mockTokens = [
  {
    id: 1,
    token_prefix: 'abc12345...',
    description: 'Test Token',
    created_at: '2026-01-20T10:00:00.000Z',
    last_used_at: '2026-01-20T11:00:00.000Z',
  },
];

export const mockTokenFull = {
  id: 1,
  token: 'abc123456789def0123456789abcdef01234567',
  description: 'Test Token',
  created_at: '2026-01-20T10:00:00.000Z',
};

export const mockUser = {
  id: 1,
  email: 'test@example.com',
  role: 'member',
  is_admin: false,
};

export const mockAdminUser = {
  id: 2,
  email: 'admin@example.com',
  role: 'admin',
  is_admin: true,
};

export const mockTeamMembers = [
  {
    id: 1,
    email: 'test@example.com',
    role: 'member',
    is_active: true,
    is_primary: false,
    created_at: '2026-01-20T10:00:00.000Z',
    last_login: '2026-01-20T11:00:00.000Z',
  },
  {
    id: 2,
    email: 'admin@example.com',
    role: 'admin',
    // Deactivated on purpose, and it is what makes the last-admin 409
    // reachable at all. `UsersService::admin_count` counts
    // `role = 'admin' AND is_active = true` (`services/users.rs:167`), so with
    // an *active* primary admin in the roster there are always two active
    // admins and the server would answer 204, never the 409 this suite
    // asserts. An admin account that was switched off, leaving one active
    // admin behind, is exactly the state the guard exists to protect.
    is_active: false,
    is_primary: true,
    created_at: '2026-01-19T10:00:00.000Z',
    last_login: null,
  },
  // A second admin who is NOT the primary account. Without this, the only admin
  // in the roster was the primary one, and `routes/team.rs:118-137` checks the
  // primary guard before the last-admin guard, so demoting them can only ever
  // produce the 403, so the 409 fixture was unreachable on the real server.
  {
    id: 3,
    email: 'second-admin@example.com',
    role: 'admin',
    is_active: true,
    is_primary: false,
    created_at: '2026-01-18T10:00:00.000Z',
    last_login: null,
  },
];

export const mockInvitations = [
  {
    token: 'invite-token-abc123',
    email: 'invitee@example.com',
    role: 'member',
    status: 'pending',
    expires_at: '2026-02-20T10:00:00.000Z',
    created_at: '2026-01-20T10:00:00.000Z',
  },
];

export const mockProjectMembers = [
  {
    user_id: 1,
    email: 'test@example.com',
    role: 'editor',
    created_at: '2026-01-20T10:00:00.000Z',
  },
  {
    user_id: 2,
    email: 'admin@example.com',
    role: 'admin',
    created_at: '2026-01-19T10:00:00.000Z',
  },
];

export const mockAlertIntegrations = [
  {
    id: 1,
    name: 'Production Webhook',
    provider_type: 'webhook',
    credentials: {
      url: 'https://example.com/webhook',
      secret: 'webhook-secret',
    },
    is_enabled: true,
    failure_count: 0,
    last_failure_at: null,
    last_failure_message: null,
    last_success_at: '2026-01-20T11:00:00.000Z',
    created_at: '2026-01-20T10:00:00.000Z',
    updated_at: '2026-01-20T10:00:00.000Z',
  },
  {
    id: 2,
    name: 'Slack Alerts',
    provider_type: 'slack',
    credentials: {
      method: 'webhook',
      webhook_url: 'https://hooks.slack.com/services/XXX',
    },
    is_enabled: true,
    failure_count: 0,
    last_failure_at: null,
    last_failure_message: null,
    last_success_at: '2026-01-20T10:30:00.000Z',
    created_at: '2026-01-19T10:00:00.000Z',
    updated_at: '2026-01-19T10:00:00.000Z',
  },
];

export const mockAlertRules = [
  {
    id: 1,
    project_id: 1,
    name: 'New Issue Alert',
    alert_type: 'new_issue',
    is_enabled: true,
    conditions: {},
    cooldown_minutes: 0,
    last_triggered_at: '2026-01-20T11:00:00.000Z',
    integration_ids: [1, 2],
    created_at: '2026-01-20T10:00:00.000Z',
    updated_at: '2026-01-20T10:00:00.000Z',
  },
  {
    id: 2,
    project_id: 1,
    name: 'Regression Alert',
    alert_type: 'regression',
    is_enabled: false,
    conditions: {},
    cooldown_minutes: 60,
    last_triggered_at: null,
    integration_ids: [1],
    created_at: '2026-01-19T10:00:00.000Z',
    updated_at: '2026-01-19T10:00:00.000Z',
  },
];

export const mockAlertHistory = [
  {
    id: 1,
    alert_rule_id: 1,
    integration_id: 1,
    issue_id: '323e4567-e89b-12d3-a456-426614174000',
    project_id: 1,
    alert_type: 'new_issue',
    channel_type: 'webhook',
    channel_name: 'Production Webhook',
    status: 'sent',
    attempt_count: 1,
    next_retry_at: null,
    error_message: null,
    http_status_code: 200,
    idempotency_key: '1-323e4567-1706183600000',
    created_at: '2026-01-20T11:00:00.000Z',
    sent_at: '2026-01-20T11:00:01.000Z',
  },
  {
    id: 2,
    alert_rule_id: 1,
    integration_id: 2,
    issue_id: '323e4567-e89b-12d3-a456-426614174000',
    project_id: 1,
    alert_type: 'new_issue',
    channel_type: 'slack',
    channel_name: 'Slack Alerts',
    status: 'failed',
    attempt_count: 3,
    next_retry_at: null,
    error_message: 'Slack API timeout',
    http_status_code: 504,
    idempotency_key: '1-323e4567-1706183600001',
    created_at: '2026-01-20T11:00:00.000Z',
    sent_at: null,
  },
];

export const handlers = [
  // ---------------------------------------------------------------------
  // Status-transform fixtures.
  //
  // These paths do NOT exist on the server. They exist so the status ->
  // error-class mapping in `src/utils/http.ts::transformHttpError` can be
  // exercised for statuses whose only real sources are endpoints this client
  // never calls:
  //
  //   - `AppError::PayloadTooLarge` is produced solely by envelope ingestion
  //     (`ingest/decompression.rs:17,38`, `ingest/parser.rs:81,107`).
  //   - The flat 429 body is produced solely by the ingest rate limiter
  //     (`routes/ingest.rs:38-46`).
  //   - `AppError::Database` / `AppError::Internal` can surface from any route,
  //     so they are parked here too rather than pinned to one arbitrary GET.
  //
  // They used to hang off `GET /api/projects/:id` under reserved ids, which
  // taught a contract the server does not honour: a bodyless GET can never
  // answer "payload too large". Drive them with a bare ky instance
  // (`createKyInstance`), not with a resource method.
  // ---------------------------------------------------------------------
  http.get(`${BASE_URL}/__status-transform/payload-too-large`, () =>
    appErrorResponse(
      'PayloadTooLarge',
      'Payload too large: Compressed payload exceeds 104857600 bytes',
    ),
  ),
  http.get(`${BASE_URL}/__status-transform/rate-limited`, () =>
    rateLimitResponse(59),
  ),
  http.get(`${BASE_URL}/__status-transform/database-error`, () =>
    appErrorResponse('DatabaseError'),
  ),
  http.get(`${BASE_URL}/__status-transform/internal-error`, () =>
    appErrorResponse('InternalError'),
  ),
  // 410 and the catch-all have no `AppError` producer at all: `status_code`
  // cannot return either. They exist for a retired endpoint and for whatever a
  // proxy invents, so both bodies are proxy-shaped rather than `appErrorResponse`.
  http.get(`${BASE_URL}/__status-transform/gone`, () =>
    HttpResponse.json(
      { error: 'This endpoint has been retired' },
      { status: 410 },
    ),
  ),
  // 418 is deliberate: it is a real status, no `AppError` maps to it, and
  // nothing in `transformHttpError`'s switch enumerates it, so it can only
  // reach the `default` arm.
  http.get(`${BASE_URL}/__status-transform/unenumerated`, () =>
    HttpResponse.json({ error: "I'm a teapot" }, { status: 418 }),
  ),

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
      digested_event_count: 0,
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
      // Mirrors the server: an omitted platform stays NULL, a supplied one is
      // persisted and echoed back.
      platform: body.platform ?? null,
    };

    return HttpResponse.json(newProject, { status: 201 });
  }),

  http.patch(`${BASE_URL}/api/projects/:id`, async ({ params, request }) => {
    const { id } = params;
    const body = (await request.json()) as {
      name?: string;
      platform?: string;
      slug?: string;
    };
    const project = mockProjects.find((p) => p.id === Number(id));

    if (!project) {
      return appErrorResponse('NotFound', projectNotFound(id));
    }

    const updated = {
      ...project,
      ...body,
      updated_at: new Date().toISOString(),
    };

    return HttpResponse.json(updated);
  }),

  http.delete(`${BASE_URL}/api/projects/:id`, ({ params }) => {
    const { id } = params;
    const project = mockProjects.find((p) => p.id === Number(id));

    if (!project) {
      return appErrorResponse('NotFound', projectNotFound(id));
    }

    return new HttpResponse(null, { status: 204 });
  }),

  // Issues
  http.get(`${BASE_URL}/api/projects/:projectId/issues`, ({ request }) => {
    const url = new URL(request.url);
    const page = parseInt(url.searchParams.get('page') ?? '1', 10);
    const q = url.searchParams.get('q');

    // Simple pagination mock - page 2 returns empty
    if (page > 1) {
      return HttpResponse.json({
        items: [],
        total_count: mockIssues.length,
        page: page,
        per_page: 20,
        total_pages: 1,
      });
    }

    const filtered = q
      ? mockIssues.filter((i) =>
          `${i.title} ${i.value} ${i.culprit}`
            .toLowerCase()
            .includes(q.toLowerCase()),
        )
      : mockIssues;

    // The list endpoint returns the lean IssueResponse: it does NOT enrich with
    // `user_report_count` (only the single-issue GET does). Mirror that here so
    // the client schema stays honest about which fields the list omits.
    const items = filtered.map(({ user_report_count: _omit, ...rest }) => rest);

    return HttpResponse.json({
      items,
      total_count: items.length,
      page: 1,
      per_page: 20,
      total_pages: 1,
    });
  }),

  // Issue sub-resources (#165): hashes, tag values, aggregates, stats
  http.get(
    `${BASE_URL}/api/projects/:projectId/issues/:issueId/hashes`,
    ({ params }) => {
      const { issueId } = params;
      return HttpResponse.json([
        {
          id: 1,
          project_id: 1,
          issue_id: issueId,
          grouping_key: 'TypeError: x ⋄ /api',
          grouping_key_hash: 'a'.repeat(64),
          created_at: '2026-01-20T10:00:00.000Z',
        },
      ]);
    },
  ),

  http.get(
    `${BASE_URL}/api/projects/:projectId/issues/:issueId/tags/:key`,
    ({ params }) => {
      const { key } = params;
      // Bare list, one entry per value (Sentry-compatible shape) — not a
      // `{key, values}` wrapper.
      return HttpResponse.json([
        {
          key,
          name: key,
          value: 'chrome',
          count: 2,
          first_seen: '2026-01-20T10:00:00.000Z',
          last_seen: '2026-01-20T11:00:00.000Z',
        },
        {
          key,
          name: key,
          value: 'firefox',
          count: 1,
          first_seen: '2026-01-20T10:30:00.000Z',
          last_seen: '2026-01-20T10:30:00.000Z',
        },
      ]);
    },
  ),

  http.get(
    `${BASE_URL}/api/projects/:projectId/issues/:issueId/aggregates`,
    () => {
      return HttpResponse.json({
        user_count: 2,
        tags: [
          {
            key: 'browser',
            total_values: 2,
            top_values: [
              { value: 'chrome', count: 2 },
              { value: 'firefox', count: 1 },
            ],
          },
        ],
      });
    },
  ),

  http.get(`${BASE_URL}/api/projects/:projectId/issues/:issueId/stats`, () => {
    return HttpResponse.json({
      data: [
        [1000, 3],
        [4600, 0],
      ],
    });
  }),

  http.get(
    `${BASE_URL}/api/projects/:projectId/issues/:issueId`,
    ({ params }) => {
      const { issueId } = params;
      const issue = mockIssues.find((i) => i.id === issueId);

      if (!issue) {
        return appErrorResponse(
          'NotFound',
          `Resource not found: Issue ${issueId} not found`,
        );
      }

      return HttpResponse.json(issue);
    },
  ),

  http.patch(
    `${BASE_URL}/api/projects/:projectId/issues/:issueId`,
    async ({ params, request }) => {
      const { issueId } = params;
      const body = (await request.json()) as {
        status?: string;
        substatus?: string;
        priority?: string;
        is_resolved?: boolean;
        is_muted?: boolean;
      };
      const issue = mockIssues.find((i) => i.id === issueId);

      if (!issue) {
        return appErrorResponse(
          'NotFound',
          `Resource not found: Issue ${issueId} not found`,
        );
      }

      // Resolve the canonical status the way the server does: `status` wins,
      // then legacy booleans; `resolvedInNextRelease` lands as `resolved`.
      let status = issue.status;
      if (body.status === 'resolvedInNextRelease') {
        status = 'resolved';
      } else if (body.status) {
        status = body.status;
      } else if (body.is_resolved === true) {
        status = 'resolved';
      } else if (body.is_resolved === false) {
        status = 'unresolved';
      } else if (body.is_muted === true) {
        status = 'ignored';
      } else if (body.is_muted === false && status === 'ignored') {
        status = 'unresolved';
      }

      const updated = {
        ...issue,
        status,
        priority: body.priority ?? issue.priority,
        is_resolved: status === 'resolved',
        is_muted: status === 'ignored',
      };

      return HttpResponse.json(updated);
    },
  ),

  http.delete(
    `${BASE_URL}/api/projects/:projectId/issues/:issueId`,
    ({ params }) => {
      const { issueId } = params;
      const issue = mockIssues.find((i) => i.id === issueId);

      if (!issue) {
        return appErrorResponse(
          'NotFound',
          `Resource not found: Issue ${issueId} not found`,
        );
      }

      return new HttpResponse(null, { status: 204 });
    },
  ),

  // Issue bulk operations (#165)
  http.put(
    `${BASE_URL}/api/projects/:projectId/issues`,
    async ({ request }) => {
      const body = (await request.json()) as { ids: string[] };
      return HttpResponse.json({ updated: body.ids.length });
    },
  ),

  http.delete(
    `${BASE_URL}/api/projects/:projectId/issues`,
    async ({ request }) => {
      const body = (await request.json()) as { ids: string[] };
      return HttpResponse.json({ deleted: body.ids.length });
    },
  ),

  // Issue activity & comments (#165)
  http.get(
    `${BASE_URL}/api/projects/:projectId/issues/:issueId/activity`,
    ({ params }) => {
      const { issueId } = params;
      return HttpResponse.json([
        {
          id: '111e4567-e89b-12d3-a456-426614174000',
          issue_id: issueId,
          user_id: 1,
          type: 'note',
          data: '{"text":"looking into it"}',
          created_at: '2026-01-20T12:00:00.000Z',
        },
      ]);
    },
  ),

  http.post(
    `${BASE_URL}/api/projects/:projectId/issues/:issueId/comments`,
    async ({ params, request }) => {
      const { issueId } = params;
      const body = (await request.json()) as { text: string };
      return HttpResponse.json(
        {
          id: '222e4567-e89b-12d3-a456-426614174000',
          issue_id: issueId,
          user_id: 1,
          type: 'note',
          data: JSON.stringify({ text: body.text }),
          created_at: '2026-01-20T12:30:00.000Z',
        },
        { status: 201 },
      );
    },
  ),

  // Bookmark / subscription / seen (#165)
  http.put(
    `${BASE_URL}/api/projects/:projectId/issues/:issueId/bookmark`,
    async ({ request }) => {
      const body = (await request.json()) as { enabled?: boolean };
      return HttpResponse.json({ is_bookmarked: body.enabled ?? true });
    },
  ),

  http.put(
    `${BASE_URL}/api/projects/:projectId/issues/:issueId/subscription`,
    async ({ request }) => {
      const body = (await request.json()) as { enabled?: boolean };
      return HttpResponse.json({ is_subscribed: body.enabled ?? true });
    },
  ),

  http.post(`${BASE_URL}/api/projects/:projectId/issues/:issueId/seen`, () =>
    HttpResponse.json({ has_seen: true }),
  ),

  // User reports (#165)
  http.get(
    `${BASE_URL}/api/projects/:projectId/issues/:issueId/user-reports`,
    ({ params }) => {
      const { issueId } = params;
      return HttpResponse.json([
        {
          id: '333e4567-e89b-12d3-a456-426614174000',
          project_id: 1,
          issue_id: issueId,
          event_id: null,
          name: 'Jane',
          email: 'jane@example.com',
          comments: 'it broke',
          created_at: '2026-01-20T13:00:00.000Z',
        },
      ]);
    },
  ),

  http.post(
    `${BASE_URL}/api/projects/:projectId/issues/:issueId/user-reports`,
    async ({ params, request }) => {
      const { issueId } = params;
      const body = (await request.json()) as {
        name?: string;
        email?: string;
        comments?: string;
      };
      return HttpResponse.json(
        {
          id: '444e4567-e89b-12d3-a456-426614174000',
          project_id: 1,
          issue_id: issueId,
          event_id: null,
          name: body.name ?? '',
          email: body.email ?? '',
          comments: body.comments ?? '',
          created_at: '2026-01-20T13:30:00.000Z',
        },
        { status: 201 },
      );
    },
  ),

  // Events
  http.get(`${BASE_URL}/api/projects/:projectId/issues/:issueId/events`, () => {
    return HttpResponse.json({
      items: mockEvents,
      has_more: false,
    });
  }),

  http.get(
    `${BASE_URL}/api/projects/:projectId/issues/:issueId/events/:eventId`,
    ({ params }) => {
      const { eventId } = params;

      if (eventId !== mockEventDetail.id) {
        return appErrorResponse(
          'NotFound',
          `Resource not found: Event ${eventId} not found`,
        );
      }

      return HttpResponse.json(mockEventDetail);
    },
  ),

  // Auth Tokens
  http.get(`${BASE_URL}/api/tokens`, () => {
    return HttpResponse.json(mockTokens);
  }),

  http.get(`${BASE_URL}/api/tokens/:id`, ({ params }) => {
    const { id } = params;
    const token = mockTokens.find((t) => t.id === Number(id));

    if (!token) {
      return appErrorResponse(
        'NotFound',
        `Resource not found: Token with id ${id} not found`,
      );
    }

    // Get by ID now returns the full token (not masked)
    return HttpResponse.json({
      id: token.id,
      token: 'abc123456789def0123456789abcdef01234567',
      description: token.description,
      created_at: token.created_at,
    });
  }),

  http.post(`${BASE_URL}/api/tokens`, async ({ request }) => {
    const body = (await request.json()) as { description?: string };

    const newToken = {
      id: 2,
      token: 'abc123456789def',
      description: body.description ?? null,
      created_at: new Date().toISOString(),
    };

    return HttpResponse.json(newToken, { status: 201 });
  }),

  http.delete(`${BASE_URL}/api/tokens/:id`, ({ params }) => {
    const { id } = params;
    const token = mockTokens.find((t) => t.id === Number(id));

    if (!token) {
      return appErrorResponse(
        'NotFound',
        `Resource not found: Token with id ${id} not found`,
      );
    }

    return new HttpResponse(null, { status: 204 });
  }),

  // Authentication
  http.get(`${BASE_URL}/auth/sso/config`, () =>
    HttpResponse.json({ enabled: true, provider_name: 'Pocket ID' }),
  ),

  http.post(`${BASE_URL}/auth/sso/start`, () =>
    HttpResponse.json(
      {
        authorization_url: 'https://id.example.com/authorize?client_id=rustrak',
      },
      {
        headers: {
          'Set-Cookie': 'rustrak_session=oidc-state; HttpOnly; SameSite=Lax',
        },
      },
    ),
  ),

  http.get(`${BASE_URL}/auth/sso/callback`, ({ request }) => {
    const url = new URL(request.url);
    if (!url.searchParams.get('state')) {
      return appErrorResponse(
        'Unauthorized',
        'Unauthorized: SSO callback is missing state',
      );
    }
    if (!url.searchParams.get('code')) {
      return appErrorResponse(
        'Unauthorized',
        'Unauthorized: SSO callback is missing a code',
      );
    }

    return HttpResponse.json(
      { user: mockUser },
      {
        headers: {
          'Set-Cookie': 'rustrak_session=authenticated; HttpOnly; SameSite=Lax',
        },
      },
    );
  }),

  //
  // `POST /auth/register` is live (`routes/auth.rs:277`) but
  // `routes/auth.rs:106-115` ignores its body entirely and always returns
  // `AppError::Forbidden("Registration is invite-only")`. There is no success
  // path and no validation branch: signing up goes through
  // `/auth/accept-invitation`. This fixture used to invent a 201 and three 400s
  // that the endpoint cannot produce.
  http.post(`${BASE_URL}/auth/register`, () =>
    appErrorResponse('Forbidden', 'Forbidden: Registration is invite-only'),
  ),

  http.post(`${BASE_URL}/auth/login`, async ({ request }) => {
    const body = (await request.json()) as {
      email: string;
      password: string;
    };

    // Check credentials
    if (body.email === 'test@example.com' && body.password === 'password123') {
      return HttpResponse.json(
        { user: mockUser },
        {
          status: 200,
          headers: {
            'Set-Cookie': 'session=mock-session-cookie; HttpOnly; SameSite=Lax',
          },
        },
      );
    }

    if (
      body.email === 'admin@example.com' &&
      body.password === 'adminpass123'
    ) {
      return HttpResponse.json(
        { user: mockAdminUser },
        {
          status: 200,
          headers: {
            'Set-Cookie': 'session=mock-session-cookie; HttpOnly; SameSite=Lax',
          },
        },
      );
    }

    // Check for inactive user
    if (body.email === 'inactive@example.com') {
      return appErrorResponse(
        'Unauthorized',
        'Unauthorized: Account is disabled',
      );
    }

    // Invalid credentials
    return appErrorResponse(
      'Unauthorized',
      'Unauthorized: Invalid credentials',
    );
  }),

  http.post(`${BASE_URL}/auth/logout`, () => {
    return new HttpResponse(null, {
      status: 204,
      headers: {
        'Set-Cookie': 'session=; Max-Age=0',
      },
    });
  }),

  http.get(`${BASE_URL}/auth/me`, ({ request }) => {
    const cookieHeader = request.headers.get('Cookie');

    // Check if session cookie is present
    if (!cookieHeader?.includes('session=mock-session-cookie')) {
      return appErrorResponse(
        'Unauthorized',
        'Unauthorized: Not authenticated',
      );
    }

    // Return current user based on session
    return HttpResponse.json(mockUser);
  }),

  // Preferences: echoes what it was given, merged onto the current user, which
  // is what the real endpoint does after writing.
  http.patch(`${BASE_URL}/auth/me`, async ({ request }) => {
    const cookieHeader = request.headers.get('Cookie');
    if (!cookieHeader?.includes('session=mock-session-cookie')) {
      return appErrorResponse(
        'Unauthorized',
        'Unauthorized: Not authenticated',
      );
    }

    const body = (await request.json()) as Record<string, unknown>;
    return HttpResponse.json({
      ...mockUser,
      language: 'language' in body ? body.language : null,
      timezone: 'timezone' in body ? body.timezone : null,
    });
  }),

  // Public invitation lookup (accept page)
  http.get(`${BASE_URL}/auth/invitation/:token`, ({ params }) => {
    const { token } = params;

    if (token === 'expired-token') {
      return appErrorResponse(
        'ValidationError',
        'Validation error: Invitation is expired or already used',
      );
    }

    const invitation = mockInvitations.find((i) => i.token === token);

    if (!invitation) {
      return appErrorResponse(
        'NotFound',
        'Resource not found: Invitation not found',
      );
    }

    return HttpResponse.json({
      email: invitation.email,
      role: invitation.role,
      status: invitation.status,
      expires_at: invitation.expires_at,
    });
  }),

  // Accept invitation (public)
  http.post(`${BASE_URL}/auth/accept-invitation`, async ({ request }) => {
    const body = (await request.json()) as {
      token: string;
      password: string;
    };

    // Guard order mirrors `services/invitation.rs:111-126`: unknown token,
    // then expired/used, then the empty password.
    if (body.token === 'invalid-token') {
      return appErrorResponse(
        'ValidationError',
        'Validation error: Invalid invitation token',
      );
    }

    if (body.token === 'expired-token') {
      return appErrorResponse(
        'ValidationError',
        'Validation error: Invitation is expired or already used',
      );
    }

    // `Password is required` is real here (invitation.rs:125), not on
    // `/auth/register` where this file used to claim it. `acceptInvitationSchema`
    // rejects an empty password client-side, so this branch documents the server
    // contract rather than a reachable client path.
    if (body.password.length === 0) {
      return appErrorResponse(
        'ValidationError',
        'Validation error: Password is required',
      );
    }

    return HttpResponse.json(
      {
        user: {
          id: 5,
          email: 'invitee@example.com',
          role: 'member',
          is_admin: false,
        },
      },
      {
        status: 201,
        headers: {
          'Set-Cookie': 'session=mock-session-cookie; HttpOnly; SameSite=Lax',
        },
      },
    );
  }),

  // Team (users)
  http.get(`${BASE_URL}/api/team`, () => {
    return HttpResponse.json(mockTeamMembers);
  }),

  http.patch(
    `${BASE_URL}/api/team/:userId/role`,
    async ({ params, request }) => {
      const { userId } = params;
      const body = (await request.json()) as { role: string };

      // Guard order mirrors `routes/team.rs:113-137` exactly, because the order
      // is observable: demoting the primary admin yields 403, never the 409.
      if (body.role !== 'admin' && body.role !== 'member') {
        return appErrorResponse(
          'ValidationError',
          `Validation error: Invalid role: ${body.role}`,
        );
      }

      const member = mockTeamMembers.find((m) => m.id === Number(userId));

      // 1. The primary account's role can never change (routes/team.rs:118-125).
      //    This is checked before the user lookup on the server, so an unknown
      //    id that happened to be primary could not exist anyway.
      if (member?.is_primary && body.role !== 'admin') {
        return appErrorResponse(
          'Forbidden',
          "Forbidden: The primary admin's role cannot be changed",
        );
      }

      // 2. Then the user must exist (routes/team.rs:129-131).
      if (!member) {
        return appErrorResponse(
          'NotFound',
          `Resource not found: User ${userId} not found`,
        );
      }

      // 3. Then the last-admin guard (routes/team.rs:134-147). The count is
      //    computed rather than assumed: the server asks
      //    `COUNT(*) WHERE role = 'admin' AND is_active = true` and only
      //    refuses at `<= 1`. Hard-coding the 409 for any non-primary admin
      //    made this fixture answer 409 where the real server answers 204,
      //    which is the fixture-vs-server divergence this phase exists to
      //    remove.
      const activeAdmins = mockTeamMembers.filter(
        (m) => m.role === 'admin' && m.is_active,
      ).length;

      if (
        member.role === 'admin' &&
        body.role === 'member' &&
        activeAdmins <= 1
      ) {
        return appErrorResponse(
          'Conflict',
          'Conflict: Cannot demote the last admin',
        );
      }

      return new HttpResponse(null, { status: 204 });
    },
  ),

  http.delete(`${BASE_URL}/api/team/:userId`, ({ params }) => {
    const { userId } = params;
    const member = mockTeamMembers.find((m) => m.id === Number(userId));
    if (!member) {
      return appErrorResponse(
        'NotFound',
        `Resource not found: User ${userId} not found`,
      );
    }
    // Simulate "the primary user cannot be deleted".
    if (member.is_primary) {
      return appErrorResponse(
        'Forbidden',
        'Forbidden: The primary admin cannot be deleted',
      );
    }
    return new HttpResponse(null, { status: 204 });
  }),

  // Invitations
  http.get(`${BASE_URL}/api/invitations`, () => {
    return HttpResponse.json(mockInvitations);
  }),

  http.post(`${BASE_URL}/api/invitations`, async ({ request }) => {
    const body = (await request.json()) as { email: string; role: string };

    // Guard order mirrors `services/invitation.rs:22-47`.
    if (body.role !== 'admin' && body.role !== 'member') {
      return appErrorResponse(
        'ValidationError',
        `Validation error: Invalid role: ${body.role}`,
      );
    }

    // `Invalid email format` is real, but it belongs to the invitation flow
    // (services/invitation.rs:26), not to `/auth/register` where this fixture
    // file used to put it.
    if (!body.email.includes('@')) {
      return appErrorResponse(
        'ValidationError',
        'Validation error: Invalid email format',
      );
    }

    if (body.email === 'existing@example.com') {
      return appErrorResponse(
        'Conflict',
        'Conflict: A user with that email already exists',
      );
    }

    if (body.email === 'pending@example.com') {
      return appErrorResponse(
        'Conflict',
        'Conflict: A pending invitation for that email already exists',
      );
    }

    return HttpResponse.json(
      {
        token: 'new-invite-token',
        email: body.email,
        role: body.role,
        status: 'pending',
        expires_at: '2026-02-20T10:00:00.000Z',
        created_at: new Date().toISOString(),
      },
      { status: 201 },
    );
  }),

  http.delete(`${BASE_URL}/api/invitations/:token`, ({ params }) => {
    const { token } = params;
    const invitation = mockInvitations.find((i) => i.token === token);

    if (!invitation) {
      return appErrorResponse(
        'NotFound',
        'Resource not found: Pending invitation not found',
      );
    }

    return new HttpResponse(null, { status: 204 });
  }),

  // Project members
  http.get(`${BASE_URL}/api/projects/:projectId/members`, ({ params }) => {
    const { projectId } = params;
    const project = mockProjects.find((p) => p.id === Number(projectId));

    if (!project) {
      // `list_members` (routes/members.rs:37) has no 404 of its own: an admin
      // actor gets an empty list for a missing project. The only 404 reachable
      // here is `access::require`'s non-member guard.
      return appErrorResponse('NotFound', projectNotVisible(projectId));
    }

    return HttpResponse.json(mockProjectMembers);
  }),

  http.put(
    `${BASE_URL}/api/projects/:projectId/members`,
    async ({ params, request }) => {
      const { projectId } = params;
      const body = (await request.json()) as {
        user_id: number;
        role: string;
      };
      const project = mockProjects.find((p) => p.id === Number(projectId));

      if (!project) {
        return appErrorResponse(
          'NotFound',
          'Resource not found: Project or user not found',
        );
      }

      if (!['viewer', 'editor', 'admin'].includes(body.role)) {
        return appErrorResponse(
          'ValidationError',
          'Validation error: Invalid role',
        );
      }

      // Simulate "cannot downgrade last project admin"
      if (body.user_id === 2 && body.role !== 'admin') {
        return appErrorResponse(
          'Conflict',
          'Conflict: Cannot downgrade the last project admin',
        );
      }

      return new HttpResponse(null, { status: 200 });
    },
  ),

  http.delete(
    `${BASE_URL}/api/projects/:projectId/members/:userId`,
    ({ params }) => {
      const { userId } = params;
      const member = mockProjectMembers.find(
        (m) => m.user_id === Number(userId),
      );

      if (!member) {
        return appErrorResponse(
          'NotFound',
          'Resource not found: Membership not found',
        );
      }

      // Simulate "cannot remove last project admin"
      if (member.user_id === 2) {
        return appErrorResponse(
          'Conflict',
          'Conflict: Cannot remove the last project admin',
        );
      }

      return new HttpResponse(null, { status: 204 });
    },
  ),

  // Alert Integrations (Global)
  http.get(`${BASE_URL}/api/integrations`, () => {
    return HttpResponse.json(mockAlertIntegrations);
  }),

  http.get(`${BASE_URL}/api/integrations/:id`, ({ params }) => {
    const { id } = params;
    const integration = mockAlertIntegrations.find((c) => c.id === Number(id));

    if (!integration) {
      return appErrorResponse(
        'NotFound',
        `Resource not found: Integration ${id} not found`,
      );
    }

    return HttpResponse.json(integration);
  }),

  http.post(`${BASE_URL}/api/integrations`, async ({ request }) => {
    const body = (await request.json()) as {
      name: string;
      provider_type: string;
      credentials: Record<string, unknown>;
      is_enabled?: boolean;
    };

    const newIntegration = {
      id: 3,
      name: body.name,
      provider_type: body.provider_type,
      credentials: body.credentials,
      is_enabled: body.is_enabled ?? true,
      failure_count: 0,
      last_failure_at: null,
      last_failure_message: null,
      last_success_at: null,
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    };

    return HttpResponse.json(newIntegration, { status: 201 });
  }),

  http.patch(
    `${BASE_URL}/api/integrations/:id`,
    async ({ params, request }) => {
      const { id } = params;
      const body = (await request.json()) as {
        name?: string;
        credentials?: Record<string, unknown>;
        is_enabled?: boolean;
      };
      const integration = mockAlertIntegrations.find(
        (c) => c.id === Number(id),
      );

      if (!integration) {
        return appErrorResponse(
          'NotFound',
          `Resource not found: Integration ${id} not found`,
        );
      }

      const updated = {
        ...integration,
        ...body,
        updated_at: new Date().toISOString(),
      };

      return HttpResponse.json(updated);
    },
  ),

  http.delete(`${BASE_URL}/api/integrations/:id`, ({ params }) => {
    const { id } = params;
    const integration = mockAlertIntegrations.find((c) => c.id === Number(id));

    if (!integration) {
      return appErrorResponse(
        'NotFound',
        `Resource not found: Integration ${id} not found`,
      );
    }

    return new HttpResponse(null, { status: 204 });
  }),

  http.post(`${BASE_URL}/api/integrations/:id/test`, ({ params }) => {
    const { id } = params;
    const integration = mockAlertIntegrations.find((c) => c.id === Number(id));

    if (!integration) {
      return appErrorResponse(
        'NotFound',
        `Resource not found: Integration ${id} not found`,
      );
    }

    return HttpResponse.json({
      success: true,
      message: 'Test notification sent successfully',
    });
  }),

  // Alert Rules (Per-Project)
  http.get(`${BASE_URL}/api/projects/:projectId/alert-rules`, ({ params }) => {
    const { projectId } = params;
    const rules = mockAlertRules.filter(
      (r) => r.project_id === Number(projectId),
    );

    return HttpResponse.json(rules);
  }),

  http.get(
    `${BASE_URL}/api/projects/:projectId/alert-rules/:ruleId`,
    ({ params }) => {
      const { projectId, ruleId } = params;
      const rule = mockAlertRules.find(
        (r) => r.project_id === Number(projectId) && r.id === Number(ruleId),
      );

      if (!rule) {
        return appErrorResponse(
          'NotFound',
          `Resource not found: Alert rule ${ruleId} not found`,
        );
      }

      return HttpResponse.json(rule);
    },
  ),

  http.post(
    `${BASE_URL}/api/projects/:projectId/alert-rules`,
    async ({ params, request }) => {
      const { projectId } = params;
      const body = (await request.json()) as {
        name: string;
        alert_type: string;
        is_enabled?: boolean;
        conditions?: Record<string, unknown>;
        cooldown_minutes?: number;
        channels?: Array<{
          integration_id: number;
          routing_override?: Record<string, unknown>;
        }>;
      };

      const integrationIds = body.channels?.map((c) => c.integration_id) ?? [];

      const newRule = {
        id: 3,
        project_id: Number(projectId),
        name: body.name,
        alert_type: body.alert_type,
        is_enabled: body.is_enabled ?? true,
        conditions: body.conditions ?? {},
        cooldown_minutes: body.cooldown_minutes ?? 0,
        last_triggered_at: null,
        integration_ids: integrationIds,
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      };

      return HttpResponse.json(newRule, { status: 201 });
    },
  ),

  http.patch(
    `${BASE_URL}/api/projects/:projectId/alert-rules/:ruleId`,
    async ({ params, request }) => {
      const { projectId, ruleId } = params;
      const body = (await request.json()) as {
        name?: string;
        is_enabled?: boolean;
        conditions?: Record<string, unknown>;
        cooldown_minutes?: number;
        channels?: Array<{
          integration_id: number;
          routing_override?: Record<string, unknown>;
        }>;
      };
      const rule = mockAlertRules.find(
        (r) => r.project_id === Number(projectId) && r.id === Number(ruleId),
      );

      if (!rule) {
        return appErrorResponse(
          'NotFound',
          `Resource not found: Alert rule ${ruleId} not found`,
        );
      }

      const integrationIds = body.channels
        ? body.channels.map((c) => c.integration_id)
        : rule.integration_ids;

      const updated = {
        ...rule,
        ...body,
        integration_ids: integrationIds,
        updated_at: new Date().toISOString(),
      };

      return HttpResponse.json(updated);
    },
  ),

  http.delete(
    `${BASE_URL}/api/projects/:projectId/alert-rules/:ruleId`,
    ({ params }) => {
      const { projectId, ruleId } = params;
      const rule = mockAlertRules.find(
        (r) => r.project_id === Number(projectId) && r.id === Number(ruleId),
      );

      if (!rule) {
        return appErrorResponse(
          'NotFound',
          `Resource not found: Alert rule ${ruleId} not found`,
        );
      }

      return new HttpResponse(null, { status: 204 });
    },
  ),

  // Alert History
  http.get(
    `${BASE_URL}/api/projects/:projectId/alert-history`,
    ({ params, request }) => {
      const { projectId } = params;
      const url = new URL(request.url);
      const limit = parseInt(url.searchParams.get('limit') ?? '50', 10);

      const history = mockAlertHistory
        .filter((h) => h.project_id === Number(projectId))
        .slice(0, limit);

      return HttpResponse.json(history);
    },
  ),

  // Source Maps — chunk upload capabilities
  http.get(
    `${BASE_URL}/api/0/organizations/:orgSlug/chunk-upload/`,
    ({ params }) => {
      const { orgSlug } = params;
      return HttpResponse.json({
        url: `http://localhost:8080/api/0/organizations/${orgSlug}/chunk-upload/`,
        chunkSize: 2097152,
        chunksPerRequest: 64,
        maxRequestSize: 33554432,
        hashAlgorithm: 'sha1',
        accept: ['artifact_bundles', 'artifact_bundles_v2'],
      });
    },
  ),

  // Source Maps — chunk upload
  http.post(
    `${BASE_URL}/api/0/organizations/:orgSlug/chunk-upload/`,
    () => new HttpResponse(null, { status: 200 }),
  ),

  // Source Maps — artifact bundle assemble
  http.post(
    `${BASE_URL}/api/0/organizations/:orgSlug/artifactbundle/assemble/`,
    async ({ request }) => {
      const body = (await request.json()) as {
        checksum: string;
        chunks: string[];
        projects: string[];
      };

      if (body.projects.length === 0) {
        return HttpResponse.json(
          { detail: 'projects array must not be empty' },
          { status: 400 },
        );
      }

      // Simulate missing chunks scenario when checksum starts with 'missing'
      if (body.checksum.startsWith('missing')) {
        return HttpResponse.json(
          {
            state: 'not_found',
            missingChunks: body.chunks.slice(0, 1),
          },
          { status: 202 },
        );
      }

      return HttpResponse.json({ state: 'ok', missingChunks: [] });
    },
  ),

  // Sessions — release health stats
  http.get(
    `${BASE_URL}/api/projects/:projectId/sessions/stats`,
    ({ params, request }) => {
      if (params.projectId === '999') {
        return appErrorResponse(
          'NotFound',
          projectNotVisible(params.projectId),
        );
      }
      const rows = [
        {
          release: '1.0.0',
          environment: 'production',
          total: 100,
          errored: 5,
          crashed: 2,
          abnormal: 1,
          healthy: 92,
          crash_free_sessions_rate: 0.98,
          crash_free_users_rate: 0.99,
        },
        {
          release: '2.0.0',
          environment: 'production',
          total: 40,
          errored: 1,
          crashed: 0,
          abnormal: 0,
          healthy: 39,
          crash_free_sessions_rate: 1,
          crash_free_users_rate: 1,
        },
      ];
      const url = new URL(request.url);
      const release = url.searchParams.get('release');
      const page = Number(url.searchParams.get('page') ?? 1);
      const perPage = Number(url.searchParams.get('per_page') ?? 20);
      const matched = release
        ? rows.filter((r) => r.release === release)
        : rows;
      const items = matched.slice((page - 1) * perPage, page * perPage);
      return HttpResponse.json({
        items,
        total_count: matched.length,
        page,
        per_page: perPage,
        total_pages: Math.ceil(matched.length / perPage),
      });
    },
  ),

  // Sessions — project-wide health summary
  http.get(
    `${BASE_URL}/api/projects/:projectId/sessions/summary`,
    ({ params }) => {
      if (params.projectId === '999') {
        return appErrorResponse(
          'NotFound',
          projectNotVisible(params.projectId),
        );
      }
      return HttpResponse.json({
        total: 300,
        errored: 15,
        crashed: 6,
        abnormal: 3,
        crash_free_sessions_rate: 0.98,
        crash_free_users_rate: 0.99,
        active_releases: 2,
      });
    },
  ),

  // Releases — new issues introduced in a release
  http.get(
    `${BASE_URL}/api/projects/:projectId/releases/:release/new-issues`,
    ({ params }) => {
      if (params.projectId === '999') {
        return appErrorResponse(
          'NotFound',
          projectNotVisible(params.projectId),
        );
      }
      return HttpResponse.json(mockIssues);
    },
  ),

  // Sessions — project-wide time-bucketed trend
  http.get(
    `${BASE_URL}/api/projects/:projectId/sessions/timeseries`,
    ({ params }) => {
      if (params.projectId === '999') {
        return appErrorResponse(
          'NotFound',
          projectNotVisible(params.projectId),
        );
      }
      return HttpResponse.json([
        {
          bucket: '2026-01-20T10:00:00.000Z',
          total: 100,
          crashed: 2,
          crash_free_sessions_rate: 0.98,
        },
        {
          bucket: '2026-01-20T11:00:00.000Z',
          total: 150,
          crashed: 3,
          crash_free_sessions_rate: 0.98,
        },
      ]);
    },
  ),

  // Stats — project-wide error volume by severity
  http.get(
    `${BASE_URL}/api/projects/:projectId/events/stats`,
    ({ params, request }) => {
      if (params.projectId === '999') {
        return appErrorResponse(
          'NotFound',
          projectNotVisible(params.projectId),
        );
      }
      const url = new URL(request.url);
      // Echo the interval back through the bucket spacing so a test can prove
      // the param reached the server.
      const interval = Number(url.searchParams.get('interval') ?? 1);
      return HttpResponse.json([
        {
          bucket: '2026-01-20T10:00:00.000Z',
          total: 10,
          fatal: 1,
          error: 6,
          warning: 2,
          info: 1,
        },
        {
          bucket:
            interval === 1
              ? '2026-01-20T11:00:00.000Z'
              : '2026-01-20T16:00:00.000Z',
          total: 0,
          fatal: 0,
          error: 0,
          warning: 0,
          info: 0,
        },
      ]);
    },
  ),

  // Stats — project counters with previous-period comparison
  http.get(
    `${BASE_URL}/api/projects/:projectId/stats/summary`,
    ({ params, request }) => {
      if (params.projectId === '999') {
        return appErrorResponse(
          'NotFound',
          projectNotVisible(params.projectId),
        );
      }
      const url = new URL(request.url);
      const period = url.searchParams.get('period');
      // All-time has no earlier window to compare against.
      if (!period) {
        return HttpResponse.json({
          period_hours: null,
          events: { current: 5000, previous: null },
          new_issues: { current: 120, previous: null },
          open_issues: 40,
        });
      }
      return HttpResponse.json({
        period_hours: 24,
        events: { current: 1200, previous: 1000 },
        new_issues: { current: 14, previous: 9 },
        open_issues: 40,
      });
    },
  ),

  // Logs — list logs for project
  http.get(`${BASE_URL}/api/projects/:projectId/logs`, ({ params }) => {
    if (params.projectId === '999') {
      return appErrorResponse('NotFound', projectNotVisible(params.projectId));
    }
    return HttpResponse.json({
      items: [
        {
          id: 'a1b2c3d4-e89b-12d3-a456-426614174000',
          trace_id: 'bbbb',
          span_id: null,
          level: 'info',
          severity_number: 9,
          body: 'ok',
          attributes: {},
          timestamp: '2026-06-18T12:00:01.000Z',
          ingested_at: '2026-06-18T12:00:02.000Z',
        },
        {
          id: 'b2c3d4e5-e89b-12d3-a456-426614174000',
          trace_id: 'aaaa',
          span_id: 'eee19b7ec3c1b174',
          level: 'error',
          severity_number: 17,
          body: 'boom',
          attributes: { 'string.attribute': { value: 'v', type: 'string' } },
          timestamp: '2026-06-18T12:00:00.000Z',
          ingested_at: '2026-06-18T12:00:02.000Z',
        },
      ],
      total_count: 2,
      page: 1,
      per_page: 20,
      total_pages: 1,
    });
  }),

  // Transactions — list transactions for project
  http.get(`${BASE_URL}/api/projects/:projectId/transactions`, ({ params }) => {
    if (params.projectId === '999') {
      return appErrorResponse('NotFound', projectNotVisible(params.projectId));
    }
    return HttpResponse.json({
      items: [
        {
          id: 'a1b2c3d4-e89b-12d3-a456-426614174000',
          event_id: 'b2c3d4e5-e89b-12d3-a456-426614174000',
          transaction_name: '/api/checkout',
          timestamp: '2026-06-18T12:00:00.000Z',
          start_timestamp: '2026-06-18T11:59:59.000Z',
          duration_ms: 1000.0,
          platform: 'javascript',
          environment: 'production',
          release: '1.0.0',
          ingested_at: '2026-06-18T12:00:01.000Z',
        },
      ],
      total_count: 1,
      page: 1,
      per_page: 20,
      total_pages: 1,
    });
  }),

  // Transactions — aggregate stats (must precede the :transactionId handler,
  // otherwise "stats" is captured as a transaction id).
  http.get(`${BASE_URL}/api/projects/:projectId/transactions/stats`, () => {
    return HttpResponse.json({
      items: [
        {
          transaction_name: '/api/checkout',
          op: 'http.server',
          count: 3,
          p50_ms: 200.0,
          p95_ms: 290.0,
          p99_ms: 298.0,
          failure_rate: 0.3333333333333333,
        },
      ],
      total_count: 1,
      page: 1,
      per_page: 20,
      total_pages: 1,
    });
  }),

  // Transactions — single group's aggregate stats
  http.get(
    `${BASE_URL}/api/projects/:projectId/transactions/stats/group`,
    ({ request }) => {
      const url = new URL(request.url);
      if (url.searchParams.get('name') === '/missing') {
        return appErrorResponse(
          'NotFound',
          `Resource not found: No stats for transaction '${url.searchParams.get('name')}'`,
        );
      }
      return HttpResponse.json({
        transaction_name: '/api/checkout',
        op: 'http.server',
        count: 3,
        p50_ms: 200.0,
        p95_ms: 290.0,
        p99_ms: 298.0,
        failure_rate: 0.3333333333333333,
      });
    },
  ),

  // Transactions — indexed spans for a transaction
  http.get(
    `${BASE_URL}/api/projects/:projectId/transactions/:transactionId/spans`,
    () => {
      return HttpResponse.json([
        {
          id: 'c3d4e5f6-e89b-12d3-a456-426614174000',
          transaction_id: 'a1b2c3d4-e89b-12d3-a456-426614174000',
          span_id: 'cccccccccccccccc',
          trace_id: 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
          parent_span_id: 'bbbbbbbbbbbbbbbb',
          op: 'db.query',
          description: 'SELECT 1',
          status: 'ok',
          start_timestamp: '2026-06-18T11:59:59.000Z',
          timestamp: '2026-06-18T11:59:59.500Z',
          duration_ms: 500.0,
          exclusive_time_ms: 500.0,
          is_segment: false,
          segment_id: null,
          platform: null,
          release: null,
          environment: null,
          gen_ai_operation_type: null,
          gen_ai_agent_name: null,
          gen_ai_request_model: null,
          gen_ai_response_model: null,
          gen_ai_tool_name: null,
          gen_ai_conversation_id: null,
          gen_ai_usage_input_tokens: null,
          gen_ai_usage_output_tokens: null,
          gen_ai_usage_total_tokens: null,
        },
      ]);
    },
  ),

  // Transactions — get single transaction detail
  http.get(
    `${BASE_URL}/api/projects/:projectId/transactions/:transactionId`,
    ({ params }) => {
      if (params.transactionId === 'missing') {
        return appErrorResponse(
          'NotFound',
          `Resource not found: Transaction ${params.transactionId} not found`,
        );
      }
      return HttpResponse.json({
        id: 'a1b2c3d4-e89b-12d3-a456-426614174000',
        event_id: 'b2c3d4e5-e89b-12d3-a456-426614174000',
        transaction_name: '/api/checkout',
        timestamp: '2026-06-18T12:00:00.000Z',
        start_timestamp: '2026-06-18T11:59:59.000Z',
        duration_ms: 1000.0,
        platform: 'javascript',
        environment: 'production',
        release: '1.0.0',
        ingested_at: '2026-06-18T12:00:01.000Z',
        data: {
          transaction: '/api/checkout',
          contexts: {
            trace: { trace_id: 'abc', span_id: 'root', op: 'http.server' },
          },
          spans: [
            {
              span_id: 'child1',
              parent_span_id: 'root',
              op: 'db',
              description: 'SELECT 1',
              start_timestamp: 1.0,
              timestamp: 1.5,
            },
          ],
          measurements: { lcp: { value: 1200.0, unit: 'millisecond' } },
          tags: { browser: 'Chrome' },
        },
      });
    },
  ),

  // Spans — list spans for project (shared table: standalone + transaction-embedded)
  http.get(`${BASE_URL}/api/projects/:projectId/spans`, ({ request }) => {
    const url = new URL(request.url);
    const operationType = url.searchParams.get('operation_type');
    const items =
      operationType && operationType !== 'agent'
        ? []
        : [
            {
              id: 'd4e5f6a7-e89b-12d3-a456-426614174000',
              transaction_id: null,
              span_id: 'eeeeeeeeeeeeeeee',
              trace_id: 'ffffffffffffffffffffffffffffffff',
              parent_span_id: null,
              op: 'gen_ai.invoke_agent',
              description: null,
              status: null,
              start_timestamp: '2026-07-16T12:00:00.000Z',
              timestamp: '2026-07-16T12:00:01.000Z',
              duration_ms: 1000.0,
              exclusive_time_ms: null,
              is_segment: true,
              segment_id: 'eeeeeeeeeeeeeeee',
              platform: null,
              release: null,
              environment: null,
              gen_ai_operation_type: 'agent',
              gen_ai_agent_name: 'planner',
              gen_ai_request_model: null,
              gen_ai_response_model: null,
              gen_ai_tool_name: null,
              gen_ai_conversation_id: null,
              gen_ai_usage_input_tokens: null,
              gen_ai_usage_output_tokens: null,
              gen_ai_usage_total_tokens: null,
            },
          ];
    return HttpResponse.json({
      items,
      total_count: items.length,
      page: 1,
      per_page: 20,
      total_pages: items.length > 0 ? 1 : 0,
    });
  }),

  // Spans — one span with its raw attribute bag (the payload the list omits)
  http.get(
    `${BASE_URL}/api/projects/:projectId/spans/:spanId`,
    ({ params }) => {
      if (params.spanId !== 'd4e5f6a7-e89b-12d3-a456-426614174000') {
        return HttpResponse.json(
          { error: { type: 'NotFound', message: 'Span not found' } },
          { status: 404 },
        );
      }
      return HttpResponse.json({
        id: 'd4e5f6a7-e89b-12d3-a456-426614174000',
        transaction_id: null,
        span_id: 'cccccccccccccccc',
        trace_id: 'ffffffffffffffffffffffffffffffff',
        parent_span_id: 'eeeeeeeeeeeeeeee',
        op: 'gen_ai.chat',
        description: 'chat claude-opus-5',
        status: 'ok',
        start_timestamp: '2026-07-16T12:00:00.000Z',
        timestamp: '2026-07-16T12:00:01.000Z',
        duration_ms: 1000.0,
        exclusive_time_ms: null,
        is_segment: false,
        segment_id: 'eeeeeeeeeeeeeeee',
        platform: null,
        release: null,
        environment: 'production',
        gen_ai_operation_type: 'ai_client',
        gen_ai_agent_name: null,
        gen_ai_request_model: 'claude-opus-5',
        gen_ai_response_model: 'claude-opus-5',
        gen_ai_tool_name: null,
        gen_ai_conversation_id: null,
        gen_ai_usage_input_tokens: 10,
        gen_ai_usage_output_tokens: 5,
        gen_ai_usage_total_tokens: 15,
        attributes: {
          'gen_ai.operation.name': 'chat',
          'gen_ai.request.model': 'claude-opus-5',
          'gen_ai.request.messages':
            '[{"role":"user","content":"what is the weather"}]',
          'gen_ai.response.text': 'it is sunny',
          'gen_ai.usage.input_tokens': 10,
          'gen_ai.usage.output_tokens': 5,
        },
        tags: { environment: 'production' },
      });
    },
  ),

  // AI Agent Monitoring — dashboard headline totals
  http.get(`${BASE_URL}/api/projects/:projectId/agents/summary`, () => {
    return HttpResponse.json({
      agent_runs: 12,
      llm_calls: 31,
      tool_calls: 18,
      error_count: 3,
      total_tokens: 48210,
      avg_duration_ms: 1840.5,
      p95_duration_ms: 4210.0,
    });
  }),

  // AI Agent Monitoring — per-model table
  http.get(`${BASE_URL}/api/projects/:projectId/agents/models`, () => {
    return HttpResponse.json([
      {
        model: 'claude-opus-5',
        requests: 18,
        errors: 2,
        avg_ms: 2100.0,
        p95_ms: 5200.0,
        input_tokens: 30000,
        cached_input_tokens: 12000,
        output_tokens: 8000,
        reasoning_output_tokens: 2400,
        total_tokens: 38000,
      },
    ]);
  }),

  // AI Agent Monitoring — per-tool table
  http.get(`${BASE_URL}/api/projects/:projectId/agents/tools/stats`, () => {
    return HttpResponse.json([
      {
        tool: 'web_search',
        calls: 14,
        errors: 1,
        avg_ms: 320.0,
        p95_ms: 910.0,
      },
    ]);
  }),

  // AI Agent Monitoring — filterable environments
  http.get(`${BASE_URL}/api/projects/:projectId/agents/environments`, () => {
    return HttpResponse.json(['production', 'staging']);
  }),

  // AI Agent Monitoring — Agent Runs over time
  http.get(`${BASE_URL}/api/projects/:projectId/agents/runs`, () => {
    return HttpResponse.json([
      { bucket: '2026-07-16T12:00:00.000Z', value: 3 },
    ]);
  }),

  // AI Agent Monitoring — Duration avg/p95 over time
  http.get(`${BASE_URL}/api/projects/:projectId/agents/duration`, () => {
    return HttpResponse.json([
      { bucket: '2026-07-16T12:00:00.000Z', avg_ms: 200.0, p95_ms: 290.0 },
    ]);
  }),

  // AI Agent Monitoring — LLM Calls by Model
  http.get(`${BASE_URL}/api/projects/:projectId/agents/models/calls`, () => {
    return HttpResponse.json([{ label: 'gpt-4o', value: 5 }]);
  }),

  // AI Agent Monitoring — Tokens Used by Model
  http.get(`${BASE_URL}/api/projects/:projectId/agents/models/tokens`, () => {
    return HttpResponse.json([{ label: 'gpt-4o', value: 1500 }]);
  }),

  // AI Agent Monitoring — Tool Calls by Tool
  http.get(`${BASE_URL}/api/projects/:projectId/agents/tools`, () => {
    return HttpResponse.json([{ label: 'search', value: 4 }]);
  }),

  // AI Agent Monitoring — Traces table
  http.get(`${BASE_URL}/api/projects/:projectId/agents/traces`, () => {
    return HttpResponse.json({
      items: [
        {
          trace_id: 'ffffffffffffffffffffffffffffffff',
          agent_names: ['planner', 'executor'],
          duration_ms: 1000.0,
          total_tokens: 150,
          tool_call_count: 1,
          llm_call_count: 2,
          error_count: 0,
          started_at: '2026-07-16T12:00:00.000Z',
        },
      ],
      total_count: 1,
      page: 1,
      per_page: 20,
      total_pages: 1,
    });
  }),

  // Source Maps — list source maps for project
  http.get(
    `${BASE_URL}/api/0/projects/:orgSlug/:projectSlug/files/source-maps/`,
    ({ params }) => {
      if (params.projectSlug === 'not-found') {
        return appErrorResponse(
          'NotFound',
          'Resource not found: project not found: not-found',
        );
      }
      return HttpResponse.json({
        data: [
          {
            debugId: 'a1b2c3d4-e5f6-7890-abcd-ef1234567890',
            fileType: 'source_map',
            size: 15234,
            timesUsed: 3,
            dateUploaded: '2026-05-22T10:00:00',
          },
        ],
      });
    },
  ),

  // Storage — instance-wide summary
  http.get(`${BASE_URL}/api/storage/summary`, () => {
    return HttpResponse.json({
      total_db_size_bytes: 1048576,
      events_count: 120,
      transactions_count: 80,
      spans_count: 640,
      logs_count: 200,
      source_maps: {
        chunk_bytes: 350,
        source_file_bytes: 300,
        total_bytes: 650,
        file_count: 2,
      },
    });
  }),

  // Storage — per-project breakdown
  http.get(`${BASE_URL}/api/storage/projects`, () => {
    return HttpResponse.json([
      {
        project_id: 1,
        project_name: 'Test Project',
        events_count: 100,
        transactions_count: 80,
        spans_count: 640,
        logs_count: 200,
        source_maps_count: 2,
        estimated_bytes: 524288,
      },
      {
        project_id: 2,
        project_name: 'Another Project',
        events_count: 0,
        transactions_count: 0,
        spans_count: 0,
        logs_count: 0,
        source_maps_count: 0,
        estimated_bytes: 0,
      },
    ]);
  }),

  // Storage — cleanup dry-run preview
  http.post(`${BASE_URL}/api/storage/cleanup/preview`, () => {
    return HttpResponse.json({
      events: 20,
      transactions: 10,
      spans: 80,
      logs: 50,
      issues_removed: 3,
    });
  }),

  // Storage — execute cleanup
  http.post(`${BASE_URL}/api/storage/cleanup`, () => {
    return HttpResponse.json({
      events: 20,
      transactions: 10,
      spans: 80,
      logs: 50,
      issues_removed: 3,
    });
  }),

  // Storage — dry-run orphaned source-map GC
  http.post(`${BASE_URL}/api/storage/source-maps/gc/preview`, () => {
    return HttpResponse.json({ files_removed: 4, bytes_freed: 81920 });
  }),

  // Storage — garbage-collect orphaned source maps
  http.post(`${BASE_URL}/api/storage/source-maps/gc`, () => {
    return HttpResponse.json({ files_removed: 4, bytes_freed: 81920 });
  }),
];
