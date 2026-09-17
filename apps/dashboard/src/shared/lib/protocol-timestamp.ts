/**
 * The grammar Relay accepts for a timestamp string, and no more.
 *
 * Relay parses a string with chrono, which takes `T`, `t` or a space between
 * the date and the time and `Z` or `z` for UTC (`parse_rfc3339_relaxed` and
 * `scan::timezone_offset` in chrono). The hour is bounded because `Date.parse`
 * treats `24:00:00` as midnight the next day, a whole day of drift on a value
 * chrono rejects outright.
 */
const ISO_DATE_TIME =
  /^(\d{4})-(\d{2})-(\d{2})[Tt ]((?:[01]\d|2[0-3]):[0-5]\d:[0-5]\d)(?:\.(\d+))?([Zz]|[+-]\d{2}:?\d{2})?$/;

/** February 30th is a date `Date.parse` rolls into March and chrono rejects. */
function isRealCalendarDate(year: number, month: number, day: number): boolean {
  const probe = new Date(0);
  probe.setUTCFullYear(year, month - 1, day);
  return (
    probe.getUTCFullYear() === year &&
    probe.getUTCMonth() === month - 1 &&
    probe.getUTCDate() === day
  );
}

/**
 * A protocol timestamp as epoch seconds, or `undefined` when it is not one.
 *
 * Every `timestamp` in the Sentry event protocol (the event's own, a span's
 * two ends, a breadcrumb's) is legally either a number of seconds since the
 * Unix epoch or an RFC 3339 string, and which one arrives depends on the SDK:
 * `@sentry/node` sends floats, `sentry-python` and `sentry-go` send strings.
 * Relay accepts both (`FromValue for Timestamp`) and re-emits a float, so
 * Sentry's own frontend only ever sees the number. Rustrak stores the payload
 * as sent, so the string reaches the browser and has to be read here, once,
 * before any arithmetic touches it: `"2016-04-20T20:55:53.845Z" * 1000` is
 * `NaN`, and a `NaN` date renders as `Invalid Date`.
 *
 * No offset means UTC, as it does in Relay. `Date.parse` would read it as the
 * reader's local time and place the same moment differently in every timezone.
 *
 * The fraction is added back by hand because `Date.parse` truncates it to
 * milliseconds, and the SDK sends microseconds: against a full-precision
 * `start_timestamp` float that truncation renders sub-millisecond spans with a
 * negative duration.
 */
export function parseEpochSeconds(raw: unknown): number | undefined {
  if (typeof raw === 'number') {
    return Number.isFinite(raw) ? raw : undefined;
  }
  if (typeof raw !== 'string') return undefined;

  const match = ISO_DATE_TIME.exec(raw);
  if (!match) return undefined;

  const [, year, month, day, time, fraction, offset] = match;
  if (!isRealCalendarDate(Number(year), Number(month), Number(day))) {
    return undefined;
  }

  // Rebuilt in the one shape ECMAScript guarantees `Date.parse` reads: an
  // uppercase `T`, and an uppercase `Z` where chrono also took a lowercase one.
  const milliseconds = Date.parse(
    `${year}-${month}-${day}T${time}${offset?.toUpperCase() ?? 'Z'}`,
  );
  if (!Number.isFinite(milliseconds)) return undefined;

  return milliseconds / 1000 + (fraction ? Number(`0.${fraction}`) : 0);
}
