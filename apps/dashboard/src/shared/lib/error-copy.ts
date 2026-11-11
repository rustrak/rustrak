import type { RustrakError } from '@rustrak/client';

/**
 * The app's own sentences for a `RustrakError`, chosen from `kind`.
 *
 * One module rather than one per surface, because a failure reads the same
 * whether it lands on a full-page card, in a single tile, a toast, or under a
 * form input.
 *
 * `network` and `server_error` carry a fixed, redacted `message` by design (the
 * real ones name the deployment's internal host, or a database error), so their
 * copy is written here. The rest carry a message the server meant a human to
 * read, and it is rendered rather than replaced.
 *
 * Every sentence is a message key resolved by the caller's translator, which
 * is required: `messages/*.json` stays the one dictionary.
 */

/** The smallest translator shape: `t(key, values)` -> string. */
export type Translate = (
  key: string,
  values?: Record<string, string | number>,
) => string;

/**
 * What went wrong, in one sentence.
 */
export function describeError(error: RustrakError, t: Translate): string {
  switch (error.kind) {
    case 'network':
      return error.reason === 'timeout'
        ? t('error.describe.networkTimeout')
        : t('error.describe.networkUnreachable');
    case 'server_error':
      return t('error.describe.serverError');
    case 'forbidden':
      return t('error.describe.forbidden');
    case 'rate_limited':
      return t('error.describe.rateLimited', {
        window: retryWindow(error.retryAfter, t),
      });
    case 'invalid_response':
      return t('error.describe.invalidResponse');
    case 'validation':
    case 'unauthenticated':
    case 'not_found':
    case 'conflict':
    case 'gone':
    case 'payload_too_large':
    case 'client_error':
    case 'invalid_request':
      return error.message;
    default: {
      // A kind added to the client fails to compile here, so it gets copy
      // (or joins the list above) on purpose rather than by default.
      const unknown: never = error;
      return (unknown as RustrakError).message;
    }
  }
}

/**
 * What the reader should do about it, or `null` when there is nothing useful
 * to add.
 *
 * By kind, because "reload once the API is back" is true for an outage and
 * misleading for everything else.
 */
export function errorGuidance(
  error: RustrakError,
  t: Translate,
): string | null {
  switch (error.kind) {
    case 'network':
    case 'server_error':
      return t('error.guidance.network');
    case 'rate_limited':
      return t('error.guidance.rateLimited');
    case 'forbidden':
      return t('error.guidance.forbidden');
    case 'invalid_response':
      return t('error.guidance.invalidResponse');
    case 'not_found':
      return t('error.guidance.notFound');
    default:
      // validation, conflict, gone, payload_too_large, client_error,
      // invalid_request, unauthenticated: the message already says everything
      // the reader can act on, and a second line would only pad it.
      return null;
  }
}

/**
 * The heading for a surface that is giving the whole viewport to this failure.
 *
 * Only the two genuine outage kinds get to claim the API is down.
 */
export function errorHeadline(error: RustrakError, t: Translate): string {
  switch (error.kind) {
    case 'network':
    case 'server_error':
      return t('error.headline.network');
    case 'forbidden':
      return t('error.headline.forbidden');
    case 'rate_limited':
      return t('error.headline.rateLimited');
    case 'invalid_response':
      return t('error.headline.invalidResponse');
    case 'not_found':
      return t('error.headline.notFound');
    default:
      return t('error.headline.other');
  }
}

/**
 * Turns `Retry-After` seconds into something a person can act on.
 *
 * The value was already parsed by the client (including the HTTP-date form) and
 * was previously dropped on the floor, so a proxy answering `Retry-After: 3600`
 * rendered as "in a moment" and the user reloaded for an hour.
 */
function retryWindow(retryAfter: number | undefined, t: Translate): string {
  if (retryAfter === undefined || retryAfter <= 0) {
    return t('error.retry.moments');
  }
  if (retryAfter < 60) {
    return t('error.retry.seconds', { count: retryAfter });
  }

  const minutes = Math.round(retryAfter / 60);
  if (minutes < 60) {
    return t('error.retry.minutes', { count: minutes });
  }

  const hours = Math.round(retryAfter / 3600);
  return t('error.retry.hours', { count: hours });
}
