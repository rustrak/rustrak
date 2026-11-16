import {
  type APIRequestContext,
  type Browser,
  test as base,
  expect,
} from '@playwright/test';
import MCR from 'monocart-coverage-reports';
import * as coverage from './coverage';

export { expect };

/**
 * Every spec imports `test` from here: it records the page's V8 coverage when
 * `E2E_COVERAGE` is set (see `coverage.ts`) and is Playwright's own otherwise.
 */
export const test = base.extend<{ recordCoverage: undefined }>({
  recordCoverage: [
    async ({ page }, use) => {
      if (!coverage.enabled) return use(undefined);
      await page.coverage.startJSCoverage({ resetOnNavigation: false });
      await use(undefined);
      await MCR(coverage.options).add(await page.coverage.stopJSCoverage());
    },
    { auto: true },
  ],
});

/**
 * Setup for the journeys: everything here goes through the API or the ingest
 * endpoints the way an SDK or a script would, so a test only drives the UI
 * for the behaviour it is about.
 */

/** The superuser `start-server.sh` creates. */
export const ADMIN = { email: 'admin@e2e.test', password: 'e2e-password' };
export const ADMIN_STATE = 'e2e/.auth/admin.json';

/** A name no other test (or retry) uses, so tests can run in parallel. */
export function unique(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
}

/** An API client signed in as the admin, for tests whose page is signed out. */
export function adminRequest(playwright: {
  request: { newContext: (o: object) => Promise<APIRequestContext> };
}) {
  return playwright.request.newContext({
    baseURL: test.info().project.use.baseURL,
    storageState: ADMIN_STATE,
  });
}

export type Project = { id: number; name: string; dsn: string };

export async function createProject(
  request: APIRequestContext,
  name = unique('e2e'),
): Promise<Project> {
  const response = await request.post('/api/projects', {
    data: { name, platform: 'python' },
  });
  expect(response.status()).toBe(201);
  return response.json();
}

const hex = (bytes: number) =>
  Array.from(crypto.getRandomValues(new Uint8Array(bytes)), (b) =>
    b.toString(16).padStart(2, '0'),
  ).join('');

/** Posts a Sentry envelope to the DSN, authenticated by its public key. */
export async function sendEnvelope(
  request: APIRequestContext,
  dsn: string,
  items: [header: object, payload: object][],
  eventId = hex(16),
) {
  const { username: key, pathname } = new URL(dsn);
  const body = [
    JSON.stringify({ event_id: eventId }),
    ...items.flatMap(([header, payload]) => [
      JSON.stringify(header),
      JSON.stringify(payload),
    ]),
  ].join('\n');
  const response = await request.post(`/api${pathname}/envelope/`, {
    headers: {
      'Content-Type': 'application/x-sentry-envelope',
      'X-Sentry-Auth': `Sentry sentry_version=7, sentry_key=${key}`,
    },
    data: body,
  });
  return response;
}

type ErrorEvent = {
  type: string;
  value: string;
  fn?: string;
  release?: string;
  /** Distinguishes events of one issue from each other. */
  user?: string;
};

/**
 * Sends an error and returns once the issue it groups into exists, because
 * events are digested in the background.
 */
export async function sendError(
  request: APIRequestContext,
  dsn: string,
  error: ErrorEvent,
): Promise<{ issueId: string; projectId: number; eventId: string }> {
  const projectId = Number(new URL(dsn).pathname.slice(1));
  const eventId = hex(16);
  const response = await sendEnvelope(
    request,
    dsn,
    [
      [
        { type: 'event' },
        {
          event_id: eventId,
          platform: 'python',
          level: 'error',
          environment: 'production',
          release: error.release,
          user: error.user ? { id: error.user } : undefined,
          exception: {
            values: [
              {
                type: error.type,
                value: error.value,
                stacktrace: {
                  frames: [
                    {
                      filename: 'app.py',
                      function: error.fn ?? 'handler',
                      lineno: 42,
                    },
                  ],
                },
              },
            ],
          },
        },
      ],
    ],
    eventId,
  );
  expect(response.ok()).toBe(true);

  // Waits for this event, not just its issue: a second event of an issue
  // that already exists would otherwise race the page.
  let issueId = '';
  await expect
    .poll(async () => {
      const event = await request.get(
        `/api/projects/${projectId}/events/sentry/${eventId}`,
      );
      issueId = event.ok() ? (await event.json()).issue_id : '';
      return issueId;
    })
    .not.toBe('');
  return { issueId, projectId, eventId };
}

/** A transaction with one span, `durationMs` long, ending now. */
export async function sendTransaction(
  request: APIRequestContext,
  dsn: string,
  txn: { name: string; durationMs: number; span: string },
) {
  const end = Date.now() / 1000;
  const start = end - txn.durationMs / 1000;
  const trace = hex(16);
  const response = await sendEnvelope(request, dsn, [
    [
      { type: 'transaction' },
      {
        event_id: hex(16),
        type: 'transaction',
        transaction: txn.name,
        platform: 'python',
        start_timestamp: start,
        timestamp: end,
        contexts: {
          trace: { trace_id: trace, span_id: hex(8), op: 'http.server' },
        },
        spans: [
          {
            span_id: hex(8),
            trace_id: trace,
            op: 'db.query',
            description: txn.span,
            start_timestamp: start,
            timestamp: end,
          },
        ],
      },
    ],
  ]);
  expect(response.ok()).toBe(true);
}

export async function sendLogs(
  request: APIRequestContext,
  dsn: string,
  lines: { level: string; body: string }[],
) {
  const response = await sendEnvelope(request, dsn, [
    [
      {
        type: 'log',
        item_count: lines.length,
        content_type: 'application/vnd.sentry.items.log+json',
      },
      {
        items: lines.map((line) => ({
          ...line,
          timestamp: Date.now() / 1000,
          trace_id: hex(16),
        })),
      },
    ],
  ]);
  expect(response.ok()).toBe(true);
}

export async function sendFeedback(
  request: APIRequestContext,
  dsn: string,
  feedback: { message: string; email: string },
) {
  const response = await sendEnvelope(request, dsn, [
    [
      { type: 'feedback' },
      {
        type: 'feedback',
        event_id: hex(16),
        timestamp: Date.now() / 1000,
        platform: 'javascript',
        contexts: {
          feedback: {
            message: feedback.message,
            contact_email: feedback.email,
          },
        },
      },
    ],
  ]);
  expect(response.ok()).toBe(true);
}

/**
 * A second person with no access to anything, created the way a real one is:
 * invited by the admin, then accepting with a password. Returns a page in
 * their own browser, signed in by the acceptance.
 */
export async function createMember(
  request: APIRequestContext,
  browser: Browser,
) {
  const email = `${unique('member')}@e2e.test`;
  const password = 'member-password';
  const invitation = await request.post('/api/invitations', {
    data: { email, role: 'member' },
  });
  expect(invitation.status()).toBe(201);
  const { token } = await invitation.json();

  const context = await freshContext(browser);
  const accepted = await context.request.post('/auth/accept-invitation', {
    data: { token, password },
  });
  expect(accepted.status()).toBe(201);
  const { user } = await accepted.json();
  return {
    email,
    password,
    id: user.id as number,
    page: await context.newPage(),
  };
}

/**
 * A browser context with no session. `browser.newContext()` inside a test
 * inherits the admin's `storageState`, and signing in from it ends the
 * admin session every other test is using.
 */
export function freshContext(browser: Browser) {
  return browser.newContext({
    baseURL: test.info().project.use.baseURL,
    storageState: { cookies: [], origins: [] },
  });
}
