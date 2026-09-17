import { describe, expect, it } from 'vitest';
import { normalizeBreadcrumbs, parseEventData } from './event-schema';

/** The two breadcrumbs an event carries after `add_breadcrumb` twice. */
function crumbs(timestamps: [unknown, unknown]) {
  return [
    {
      category: 'auth',
      level: 'info',
      message: 'user logged in',
      timestamp: timestamps[0],
      type: 'default',
    },
    {
      category: 'http',
      data: { status_code: 200 },
      level: 'info',
      message: 'GET /api/items',
      timestamp: timestamps[1],
      type: 'default',
    },
  ];
}

describe('breadcrumb timestamps', () => {
  // Captured from sentry-python 2.69.2 against a Rustrak server: the SDK
  // serializes every `datetime` with `strftime("%Y-%m-%dT%H:%M:%S.%fZ")` and
  // wraps the list in `values`.
  it('reads the RFC 3339 strings sentry-python sends into epoch seconds', () => {
    const parsed = parseEventData({
      breadcrumbs: {
        values: crumbs([
          '2026-09-17T23:21:54.326158Z',
          '2026-09-17T23:21:54.326185Z',
        ]),
      },
    });
    const breadcrumbs = normalizeBreadcrumbs(parsed.breadcrumbs);

    expect(breadcrumbs).toHaveLength(2);
    expect(breadcrumbs[0]?.timestamp).toBe(1789687314 + 0.326158);
    expect(breadcrumbs[1]?.timestamp).toBe(1789687314 + 0.326185);
    // Everything else on the crumb survives the read untouched.
    expect(breadcrumbs[1]).toMatchObject({
      category: 'http',
      data: { status_code: 200 },
      message: 'GET /api/items',
    });
  });

  // Captured from @sentry/node 10.75.0 against the same server: a bare list
  // of crumbs, each stamped by `dateTimestampInSeconds()`.
  it('passes the epoch floats @sentry/node sends through unchanged', () => {
    const parsed = parseEventData({
      breadcrumbs: crumbs([1789687315.213, 1789687315.214]),
    });
    const breadcrumbs = normalizeBreadcrumbs(parsed.breadcrumbs);

    expect(breadcrumbs.map((crumb) => crumb.timestamp)).toEqual([
      1789687315.213, 1789687315.214,
    ]);
  });

  it('keeps a crumb whose timestamp it cannot read, as Relay does', () => {
    const parsed = parseEventData({
      breadcrumbs: crumbs(['yesterday-ish', null]),
    });
    const breadcrumbs = normalizeBreadcrumbs(parsed.breadcrumbs);

    expect(breadcrumbs).toHaveLength(2);
    expect(breadcrumbs[0]?.timestamp).toBeUndefined();
    expect(breadcrumbs[1]?.timestamp).toBeUndefined();
    expect(breadcrumbs[0]?.message).toBe('user logged in');
  });

  it('does not let one string timestamp hide the whole trail', () => {
    const parsed = parseEventData({
      breadcrumbs: crumbs([1789687315.213, '2026-09-17T23:21:55.214Z']),
    });

    expect(parsed.breadcrumbs).toBeDefined();
    expect(normalizeBreadcrumbs(parsed.breadcrumbs)).toHaveLength(2);
  });

  it('reads no breadcrumbs as an empty trail', () => {
    expect(normalizeBreadcrumbs(undefined)).toEqual([]);
    expect(normalizeBreadcrumbs({})).toEqual([]);
  });
});
