import { useMemo } from 'react';
import {
  createFormatter,
  type Formats,
  type useFormatter,
  useLocale,
  useTimeZone,
} from 'use-intl';
import { FALLBACK_TIME_ZONE, formatsFor } from '@/shared/i18n/config';

/** `useFormatter()` without `relativeTime`: what a table's columns may close over. */
export type StableFormatter = Omit<
  ReturnType<typeof useFormatter>,
  'relativeTime'
>;

/**
 * `useFormatter()` changes with `now`, after every read, and new `cell`
 * functions remount every table cell. Numbers and dates do not read `now`, so
 * this one changes only with the locale and the zone. Render relative times
 * with `RelativeTime`.
 */
export function useStableFormatter(): StableFormatter {
  const locale = useLocale();
  const timeZone = useTimeZone();
  return useMemo(() => {
    const { relativeTime: _, ...rest } = createFormatter({
      locale,
      timeZone,
      formats: formatsFor(timeZone ?? FALLBACK_TIME_ZONE) as Formats,
    });
    return rest;
  }, [locale, timeZone]);
}
