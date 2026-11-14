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
 * What a table's columns close over: new `cell` functions remount every
 * cell, so it changes only with the locale and the zone. It has no
 * `relativeTime`, whose `now` moves after every read: render relative times
 * with `RelativeTime`, which re-renders on its own.
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
