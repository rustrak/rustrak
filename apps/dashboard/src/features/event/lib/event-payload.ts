import {
  normalizeBreadcrumbs,
  normalizeThreads,
  parseEventData,
} from '@/features/event/lib/event-schema';

/**
 * An event's payload, parsed once, with the answer to "does this event have
 * anything to show in section X?" already worked out.
 *
 * These flags used to be nine `const has…` lines at the top of the page
 * component, each one an `Object.keys(...).length > 0` or a `Boolean(...)`
 * spelled slightly differently. They are a property of the payload, not of the
 * page: the same event rendered anywhere else has the same sections.
 */
export function readEventPayload(eventData: Record<string, unknown>) {
  const parsed = parseEventData(eventData);
  const breadcrumbs = normalizeBreadcrumbs(parsed.breadcrumbs);
  const threads = normalizeThreads(parsed.threads);
  const tags = parsed.tags ?? {};

  // A `threads` entry supersedes a bare `exception` view -- the same dispatch
  // Sentry's frontend uses. Crashes reported via threads carry their own
  // exception cross-linking, so the plain exception view would be a strictly
  // worse rendering of the same data.
  const hasThreads = threads.length > 0;

  return {
    ...parsed,
    breadcrumbs,
    threads,
    tags,
    has: {
      threads: hasThreads,
      stackTrace: hasThreads || Boolean(parsed.exception?.values?.length),
      breadcrumbs: breadcrumbs.length > 0,
      contexts: Boolean(
        parsed.contexts && Object.keys(parsed.contexts).length > 0,
      ),
      modules: Boolean(
        parsed.modules && Object.keys(parsed.modules).length > 0,
      ),
      user: Boolean(
        parsed.user &&
          (parsed.user.id || parsed.user.email || parsed.user.ip_address),
      ),
      tags: Object.keys(tags).length > 0,
    },
  };
}

/**
 * The exception type and the full message, split the way Sentry splits them.
 *
 * An issue title is `Type: value` when the SDK sent both; everything before
 * the first `: ` is the type, and a title with no colon is the type on its
 * own.
 */
export function splitIssueTitle(title: string, value?: string | null) {
  const colonIdx = title.indexOf(': ');
  return {
    type: colonIdx > 0 ? title.slice(0, colonIdx) : title,
    message: value || title,
  };
}

/** A payload record with at least one key. SDKs may send `null` for any of them. */
export function isNonEmpty<T extends object>(
  record: T | null | undefined,
): record is T {
  return record != null && Object.keys(record).length > 0;
}
