import type { IssueResponse } from '@rustrak/client';
import { resolveAgeDuration } from '@/shared/lib/resolve-age';

/**
 * Which release a resolved issue was resolved in, read from its
 * `status_details` as Sentry's group serializer writes them: `inNextRelease`
 * until the next release is known, then `inRelease`. A plain resolve has none.
 */
export type Resolution =
  | { kind: 'nextRelease' }
  | { kind: 'release'; version: string };

export function resolutionOf(
  issue: Pick<IssueResponse, 'status' | 'status_details'>,
): Resolution | null {
  if (issue.status !== 'resolved') return null;
  const details = issue.status_details;
  if (typeof details?.inRelease === 'string' && details.inRelease) {
    return { kind: 'release', version: details.inRelease };
  }
  if (details?.inNextRelease) return { kind: 'nextRelease' };
  return null;
}

/** A message key and its values, for the component to translate. */
export interface Message {
  key: string;
  values?: Record<string, string | number>;
}

const text = (value: unknown) =>
  typeof value === 'string' && value ? value : null;

/**
 * Sentry's three phrasings of a "resolved in release" activity: above the
 * release that was current (semver), in a known release, or in the upcoming
 * one while it is not known yet.
 */
export function resolvedInReleaseMessage(
  data: Record<string, unknown>,
): Message {
  const current = text(data.current_release_version);
  if (current) {
    return {
      key: 'activity.resolvedAfterRelease',
      values: { version: current },
    };
  }
  const version = text(data.version);
  if (version) {
    return { key: 'activity.resolvedInRelease', values: { version } };
  }
  return { key: 'activity.resolvedInNextRelease' };
}

/** A regression, and the release it came back in when the event had one. */
export function regressionMessage(data: Record<string, unknown>): Message {
  const version = text(data.version);
  return version
    ? { key: 'activity.regressionInRelease', values: { version } }
    : { key: 'activity.regression' };
}

/**
 * Auto resolve's activity (Sentry's `SET_RESOLVED_BY_AGE`): how long the
 * issue went without an event, or just inactivity when the age is missing.
 */
export function resolvedByAgeMessage(data: Record<string, unknown>): Message {
  const age = Number(data.age);
  if (!Number.isInteger(age) || age <= 0)
    return { key: 'activity.resolvedByAge' };
  const { unit, count } = resolveAgeDuration(age);
  return {
    key:
      unit === 'days'
        ? 'activity.resolvedByAgeDays'
        : 'activity.resolvedByAgeHours',
    values: { count },
  };
}
