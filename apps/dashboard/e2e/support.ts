import {
  type APIRequestContext,
  type Browser,
  type BrowserContext,
  test as base,
  expect,
  type Page,
} from '@playwright/test';
import MCR from 'monocart-coverage-reports';
import * as coverage from './coverage';

export { expect };

/** Pages opened in other users' contexts (`newPage`) during the current test. */
const otherPages: Page[] = [];

/**
 * Every spec imports `test` from here: it records V8 coverage when
 * `E2E_COVERAGE` is set (see `coverage.ts`) and is Playwright's own otherwise.
 * That is the test's `page` and every page `newPage` opened, whose contexts it
 * closes afterwards, once their coverage is in.
 */
export const test = base.extend<{ recordCoverage: undefined }>({
  recordCoverage: [
    async ({ page }, use) => {
      if (coverage.enabled)
        await page.coverage.startJSCoverage({ resetOnNavigation: false });
      await use(undefined);
      const pages = [page, ...otherPages.splice(0)];
      for (const each of pages) {
        if (coverage.enabled && !each.isClosed())
          await MCR(coverage.options).add(await each.coverage.stopJSCoverage());
        if (each !== page) await each.context().close();
      }
    },
    { auto: true },
  ],
});

/**
 * A page in another user's context. Use it rather than `context.newPage()`,
 * and leave closing to the fixture, so its coverage is counted.
 */
export async function newPage(context: BrowserContext) {
  const page = await context.newPage();
  otherPages.push(page);
  if (coverage.enabled)
    await page.coverage.startJSCoverage({ resetOnNavigation: false });
  return page;
}

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
  // The DSN's own origin, not the suite's: a DSN naming the wrong server must
  // fail here the way it would fail for an SDK.
  const { username: key, pathname, origin } = new URL(dsn);
  const body = [
    JSON.stringify({ event_id: eventId }),
    ...items.flatMap(([header, payload]) => [
      JSON.stringify(header),
      JSON.stringify(payload),
    ]),
  ].join('\n');
  const response = await request.post(`${origin}/api${pathname}/envelope/`, {
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
  lines: { level: string; body: string; environment?: string }[],
) {
  const response = await sendEnvelope(request, dsn, [
    [
      {
        type: 'log',
        item_count: lines.length,
        content_type: 'application/vnd.sentry.items.log+json',
      },
      {
        items: lines.map(({ environment, ...line }) => ({
          ...line,
          timestamp: Date.now() / 1000,
          trace_id: hex(16),
          // Where every SDK puts it: a typed attribute.
          ...(environment && {
            attributes: {
              'sentry.environment': { value: environment, type: 'string' },
            },
          }),
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

  // Stored in the background, like an error: the inbox does not poll.
  const projectId = Number(new URL(dsn).pathname.slice(1));
  await expect
    .poll(async () => {
      const { items } = await (
        await request.get(
          `/api/projects/${projectId}/issues?category=feedback&filter=all`,
        )
      ).json();
      return items.some((item: Record<string, unknown>) =>
        Object.values(item).some(
          (value) =>
            typeof value === 'string' && value.includes(feedback.message),
        ),
      );
    })
    .toBe(true);
}

/**
 * A second person, with no access to anything unless `role` gives it, created the way a real one is:
 * invited by the admin, then accepting with a password. Returns a page in
 * their own browser, signed in by the acceptance.
 */
export async function createMember(
  request: APIRequestContext,
  browser: Browser,
  role = 'member',
) {
  const email = `${unique('member')}@e2e.test`;
  const password = 'member-password';
  const invitation = await request.post('/api/invitations', {
    data: { email, role },
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
    page: await newPage(context),
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
