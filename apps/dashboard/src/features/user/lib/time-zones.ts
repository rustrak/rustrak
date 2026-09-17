/** One entry in the timezone picker. */
export interface TimeZoneOption {
  /** The IANA name, which is what `users.timezone` stores. `Europe/Madrid`. */
  value: string;
  /** Minutes east of UTC at `now`, so the list can sort west to east. */
  offsetMinutes: number;
  /** The offset as a reader scans for it: `UTC+02:00`, or `UTC` at zero. */
  offset: string;
}

/**
 * Minutes east of UTC that `timeZone` observes at `now`.
 *
 * Read off the zone's own wall clock rather than a `timeZoneName` part:
 * `longOffset` is the newer of the two APIs and the wall clock is what every
 * runtime that has `Intl` at all can already print. The zone's civil time at
 * `now` is reassembled as if it were UTC, and the distance to the real `now`
 * is the offset, DST included.
 */
export function timeZoneOffsetMinutes(timeZone: string, now: Date): number {
  // Not a display format, which is why the locale is pinned rather than the
  // reader's: the parts come back as ASCII digits to be read as numbers, and
  // nothing produced here reaches the screen. The reader's locale formats
  // the resulting zone through `useFormatter` like every other date.
  const parts = Intl.DateTimeFormat('en-US', {
    timeZone,
    hourCycle: 'h23',
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  }).formatToParts(now);
  const field = (type: Intl.DateTimeFormatPartTypes) =>
    Number(parts.find((part) => part.type === type)?.value);

  const asUtc = Date.UTC(
    field('year'),
    field('month') - 1,
    field('day'),
    field('hour'),
    field('minute'),
    field('second'),
  );
  // Milliseconds are below the formatter's resolution, so they are dropped
  // from both sides before the subtraction.
  const truncated = now.getTime() - now.getUTCMilliseconds();
  return Math.round((asUtc - truncated) / 60_000);
}

/** `UTC+02:00`, `UTC-03:30`, or plain `UTC` for zero, the way Sentry labels a zone. */
export function formatOffset(offsetMinutes: number): string {
  if (offsetMinutes === 0) return 'UTC';
  const sign = offsetMinutes < 0 ? '-' : '+';
  const absolute = Math.abs(offsetMinutes);
  const hours = String(Math.floor(absolute / 60)).padStart(2, '0');
  const minutes = String(absolute % 60).padStart(2, '0');
  return `UTC${sign}${hours}:${minutes}`;
}

/**
 * Every zone this runtime can format with, as picker options.
 *
 * The list is the runtime's own (`Intl.supportedValuesOf`), which is also the
 * list `resolveTimeZone` validates a stored value against, so nothing here can
 * be offered that would later be read as "unknown, fall back to UTC". It is
 * not a hand-maintained table for the same reason the server keeps none: the
 * tz database changes on its own schedule.
 *
 * Sorted west to east and then by name, so scanning for "my offset" works
 * without knowing the name, and `UTC` itself is always present because
 * choosing it on purpose is half the point of the control.
 */
export function listTimeZones(now: Date = new Date()): TimeZoneOption[] {
  let names: string[];
  try {
    names = Intl.supportedValuesOf('timeZone');
  } catch {
    names = [];
  }
  if (!names.includes('UTC')) names = [...names, 'UTC'];

  return names
    .flatMap((value) => {
      try {
        const offsetMinutes = timeZoneOffsetMinutes(value, now);
        return [{ value, offsetMinutes, offset: formatOffset(offsetMinutes) }];
      } catch {
        // A name the enumerator lists but the formatter rejects would throw
        // from every date on the page once chosen; leave it out.
        return [];
      }
    })
    .sort(
      (a, b) =>
        a.offsetMinutes - b.offsetMinutes || a.value.localeCompare(b.value),
    );
}
