import { useMemo } from 'react';
import { useFormatter, useLocale, useTimeZone } from 'use-intl';

/** `useFormatter()` without `relativeTime`: what a table's columns may close over. */
export type ColumnFormatter = Omit<
  ReturnType<typeof useFormatter>,
  'relativeTime'
>;

/**
 * A formatter that keeps its identity when `now` moves.
 *
 * `useFormatter()` returns a new formatter each time the provider's `now`
 * moves, which is after every successful read. A table that builds its
 * columns from it builds new `cell` functions each time, and `flexRender`
 * treats a new function as a new component: every cell remounts, and a menu
 * that was open closes, and the focus it gave back falls to the page.
 *
 * Numbers and dates do not read `now`, so this one changes only with the
 * locale and the zone. The named formats are derived from the zone
 * (`formatsFor`), so the zone stands for them. A relative time reads `now`,
 * so it is left out, at runtime as well as in the type: render it with
 * `RelativeTime`, which follows `now` on its own.
 */
export function useColumnFormatter(): ColumnFormatter {
  const format = useFormatter();
  const locale = useLocale();
  const timeZone = useTimeZone();
  // `format` is left out on purpose: it changes with `now`, which nothing
  // here reads. The memo would otherwise change on every read.
  // react-doctor-disable-next-line react-doctor/exhaustive-deps
  return useMemo(() => withoutRelativeTime(format), [locale, timeZone]);
}

function withoutRelativeTime(
  format: ReturnType<typeof useFormatter>,
): ColumnFormatter {
  const { relativeTime: _stale, ...rest } = format;
  return rest;
}
