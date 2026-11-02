import { useFormatter } from 'use-intl';

/**
 * "3 minutes ago", read against the provider's `now`, in a `<time>` element
 * that carries the exact instant for machines and assistive technology.
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
   */
  style?: Intl.RelativeTimeFormatStyle;
}) {
  const format = useFormatter();
  const date = new Date(at);
  const text = format.relativeTime(date, { style });
  // `toISOString()` throws on an invalid date; the text alone is still right.
  if (Number.isNaN(date.getTime())) return <>{text}</>;
  return <time dateTime={date.toISOString()}>{text}</time>;
}
