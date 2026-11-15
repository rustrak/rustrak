/**
 * The pure half of the i18n setup: which languages, which one and which zone
 * for a reader, and how the named formats read once the zone is known.
 *
 * The locale is not in the URL: a link pasted to a colleague opens in the
 * reader's language, from `users.language` once they have chosen and the
 * browser's before that. No cookie: a preference on the account follows the
 * reader to any browser.
 */

export const LOCALES = ['en', 'zh', 'fr', 'es', 'ro'] as const;

export type Locale = (typeof LOCALES)[number];

const DEFAULT_LOCALE: Locale = 'en';

/** Whether `value` is a language this app actually has messages for. */
export function isLocale(value: string | undefined): value is Locale {
  return value !== undefined && (LOCALES as readonly string[]).includes(value);
}

/**
 * Where a reader with no stored zone lands.
 *
 * UTC rather than a guess: it is the zone the events arrive in and the one the
 * logs a reader correlates against are already written in. `TimeZoneSync`
 * replaces it with the browser's on the first authenticated render.
 */
export const FALLBACK_TIME_ZONE = 'UTC';

/**
 * Which language to answer in.
 *
 * The order is the reader's choice first, then their browser's, then English:
 *
 * 1. **`users.language`**, chosen on `/settings/account`. It lives on the
 *    account rather than in a cookie so it follows the reader to any browser
 *    they log in from, and survives clearing site data.
 * 2. **The browser's languages**, which is all there is before anyone is
 *    authenticated — the login, the invitation and the 404 all render with no
 *    user to read a preference from. This is `navigator.languages`, the same
 *    list the browser used to send as `Accept-Language`, already in preference
 *    order. Matched on the base tag, so `zh-CN`, `zh-TW` and `zh-Hans` all
 *    reach `zh`.
 * 3. **English**, because a dashboard has to render something.
 *
 * The stored value is validated rather than trusted: the server accepts any
 * well-formed language tag by design (it has no business knowing which ones
 * this dashboard ships), so `pt-BR` can legitimately be in the column while
 * there are no Portuguese messages to render.
 */
export function resolveLocale(
  preferred: string | null | undefined,
  accepted: readonly string[],
): Locale {
  if (isLocale(preferred ?? undefined)) return preferred as Locale;

  for (const tag of accepted) {
    const base = tag.split('-')[0].trim().toLowerCase();
    const match = LOCALES.find((locale) => locale === base);
    if (match) return match;
  }

  return DEFAULT_LOCALE;
}

/** Whether `value` is a zone this runtime can actually format with. */
function isValidTimeZone(value: string): boolean {
  try {
    return Intl.supportedValuesOf('timeZone').includes(value);
  } catch {
    return false;
  }
}

/**
 * The reader's zone, or {@link FALLBACK_TIME_ZONE} when they have none.
 *
 * Validated rather than trusted: the API accepts any well-formed zone name by
 * design, and `Intl` throws `RangeError` on a name it does not know, which in
 * here would take down every component that renders a date.
 */
export function resolveTimeZone(stored: string | null | undefined): string {
  if (!stored || !isValidTimeZone(stored)) return FALLBACK_TIME_ZONE;
  return stored;
}

/**
 * The two clocks a reader can ask for. `h23` rather than `hour12: false`,
 * which some engines read as `h24` and render midnight as "24:05".
 */
export type HourCycle = 'h12' | 'h23';

/**
 * The reader's clock, or `undefined` to keep the one their language uses.
 *
 * `users.clock_24h` is three states, and the third is the point: a reader who
 * never opened the setting keeps the 24-hour clock in French and the 12-hour
 * one in English, which is what the language's own formats already do.
 */
export function resolveHourCycle(
  clock24h: boolean | null | undefined,
): HourCycle | undefined {
  if (clock24h === true) return 'h23';
  if (clock24h === false) return 'h12';
  return undefined;
}

/**
 * The option sets, named once.
 *
 * These replace 8 distinct `date-fns` pattern strings ('PPpp', 'MMM d',
 * 'MMM d, HH:mm', ...) that were spread across 19 files and drifting: the
 * same kind of value read three different ways depending on which component
 * rendered it. A name here also means a call site says *what* it is showing
 * rather than how, so a change to how a chart axis reads is one edit.
 *
 * Sentry's rule decides the zone suffix, and it is a good one. `dateTime.tsx`
 * shows the zone only when the time is UTC, on the reasoning that "the user
 * would want to know that it's UTC and not their own time zone". A local time
 * needs no label because it matches the reader's own clock; a UTC time looks
 * exactly like a local one and is silently two hours out. The suffix appears
 * precisely where it would otherwise mislead.
 *
 * Here UTC means one of two things, and both want the label: the reader has
 * not told us their zone yet, or they genuinely run on UTC.
 *
 * `hourCycle` is the reader's clock (`users.clock_24h`), spread into every
 * format that shows an hour and left out when they never chose one, so the
 * language decides. Only here: a call site that writes its own options
 * instead of naming one of these ignores the setting.
 */
export function formatsFor(timeZone: string, hourCycle?: HourCycle) {
  const showZone = timeZone === FALLBACK_TIME_ZONE;
  const clock = hourCycle ? { hourCycle } : {};

  return {
    dateTime: {
      /**
       * A calendar day. "Jan 5, 2026" / "2026年1月5日".
       *
       * No zone suffix even under `showZone`: a date with no time on it is
       * not a clock reading, and "Jan 5, 2026 UTC" invites the reader to
       * wonder which day it is for them.
       */
      date: { year: 'numeric', month: 'short', day: 'numeric' },
      /** A day and a wall-clock time, the default for a timestamp. */
      dateTime: {
        dateStyle: 'medium',
        timeStyle: showZone ? 'long' : 'short',
        ...clock,
      },
      /** With seconds, for log lines and event ingestion stamps. */
      precise: {
        dateStyle: 'medium',
        timeStyle: showZone ? 'long' : 'medium',
        ...clock,
      },
      /** A log line's clock: to the millisecond, since lines come in bursts. */
      logLine: {
        month: 'short',
        day: 'numeric',
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit',
        fractionalSecondDigits: 3,
        ...clock,
      },
      /** Time only, for a breadcrumb inside one event. */
      time: {
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit',
        ...(showZone ? { timeZoneName: 'short' as const } : {}),
        ...clock,
      },
      /** A chart tick where the bucket is a day or wider. */
      axisDay: { month: 'short', day: 'numeric' },
      /** A tick for a bucket that is a UTC day, read as that day anywhere. */
      axisDayUtc: { month: 'short', day: 'numeric', timeZone: 'UTC' },
      /**
       * A chart's hourly bucket: the day and the hour. With the minutes,
       * though they are always `:00`: on a 24-hour clock the hour alone
       * reads "Nov 14, 22", a bare number.
       */
      axisHour: {
        month: 'short',
        day: 'numeric',
        hour: 'numeric',
        minute: '2-digit',
        ...clock,
      },
      /** A chart tick where the bucket is narrower than a day. */
      axisTime: {
        month: 'short',
        day: 'numeric',
        hour: 'numeric',
        minute: '2-digit',
        ...clock,
      },
      /** The hour and minute alone, for the start of a short timeline. */
      clock: { hour: 'numeric', minute: '2-digit', ...clock },
      /**
       * A scheduled run: which weekday, then when. The call site passes the
       * monitor's own `timeZone`, since a schedule is read in the zone it
       * runs in.
       */
      scheduledRun: {
        weekday: 'short',
        month: 'short',
        day: 'numeric',
        hour: '2-digit',
        minute: '2-digit',
        ...clock,
      },
    },
    number: {
      /** `12403` -> "12.4K" in English, "1.2万" in Chinese. */
      compact: { notation: 'compact', maximumFractionDigits: 1 },
      /** `0.183` -> "18.3%". A rate, where a sign would be meaningless. */
      percent: { style: 'percent', maximumFractionDigits: 1 },
      /** `0.183` -> "+18.3%". Signed, because the sign is the point. */
      percentChange: {
        style: 'percent',
        signDisplay: 'exceptZero',
        maximumFractionDigits: 1,
      },
    },
  } as const;
}
