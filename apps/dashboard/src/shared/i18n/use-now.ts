import { useCallback, useSyncExternalStore } from 'react';
import { useFormatter } from 'use-intl';
import { clock } from './clock';

/** The shared `now` (see `clock.ts`). Re-renders the caller when it moves. */
export function useNow(): Date {
  return useSyncExternalStore(clock.subscribe, clock.now, clock.now);
}

/**
 * `format.relativeTime`, read against the shared `now`. The only way to write
 * "3 minutes ago": the intl provider carries no `now`
 * (`clock-outside-intl.test.ts`).
 */
export function useRelativeTime(): (
  date: Date | number,
  style?: Intl.RelativeTimeFormatStyle,
) => string {
  const format = useFormatter();
  const now = useNow();
  return useCallback(
    (date, style) => format.relativeTime(date, { now, style }),
    [format, now],
  );
}
