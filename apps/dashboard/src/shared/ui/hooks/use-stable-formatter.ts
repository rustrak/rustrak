import { useMemo, useSyncExternalStore } from 'react';
import { createFormatter, type useFormatter } from 'use-intl';
import { intl } from '@/shared/i18n/intl';

/** `useFormatter()` without `relativeTime`: what a table's columns may close over. */
export type StableFormatter = Omit<
  ReturnType<typeof useFormatter>,
  'relativeTime'
>;

/**
 * What a table's columns close over: new `cell` functions remount every
 * cell, so it changes only when the reader's language, zone or clock does.
 * It has no `relativeTime`, whose `now` moves after every read: render
 * relative times with `RelativeTime`, which re-renders on its own.
 *
 * Built from the same snapshot `IntlProvider` renders from, formats included,
 * so a table cannot read a date differently from the page around it. Keyed on
 * the snapshot, which only `intl.reload()` replaces, after the reader saved a
 * preference.
 */
export function useStableFormatter(): StableFormatter {
  const snapshot = useSyncExternalStore(
    intl.subscribe,
    intl.snapshot,
    intl.snapshot,
  );
  return useMemo(() => {
    const { relativeTime: _, ...rest } = createFormatter({
      locale: snapshot.locale,
      timeZone: snapshot.timeZone,
      formats: snapshot.formats,
    });
    return rest;
  }, [snapshot]);
}
