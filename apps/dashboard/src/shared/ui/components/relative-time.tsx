import { useRelativeTime } from '@/shared/i18n/use-now';

/**
 * "3 minutes ago", read against the shared `now` (`shared/i18n/clock.ts`), in
 * a `<time>` element that carries the exact instant for machines and
 * assistive technology.
 *
 * A component rather than a call inside a table's `cell`, so the cell
 * function can stay the same when `now` moves: this re-renders with the new
 * `now` by itself (see `useStableFormatter`).
 */
export function RelativeTime({
  at,
  style,
}: {
  /** An ISO 8601 string, as the API sends it. */
  at: string;
  /**
   * The length of the words: `long` ("3 minutes ago", the default), `short`
   * or `narrow`. It is `Intl.RelativeTimeFormat`'s `style` option, not CSS.
   * Prefer `short` for tight space: `narrow` drops the phrasing in some
   * locales (French "-3 min" for "il y a 3 min").
   */
  style?: Intl.RelativeTimeFormatStyle;
}) {
  const relativeTime = useRelativeTime();
  const date = new Date(at);
  const text = relativeTime(date, style);
  // `toISOString()` throws on an invalid date; the text alone is still right.
  if (Number.isNaN(date.getTime())) return <>{text}</>;
  return <time dateTime={date.toISOString()}>{text}</time>;
}
