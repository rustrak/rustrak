import { Switch } from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { useId, useSyncExternalStore } from 'react';
import { toast } from 'sonner';
import { useFormatter, useTranslations } from 'use-intl';
import { userMutations } from '@/features/user/api/mutations';
import { session } from '@/shared/api/session';
import { intl } from '@/shared/i18n/intl';
import { SettingRow } from '@/shared/ui/components/setting-row';

/**
 * The clock on the account row right now, or `null` when it holds none.
 *
 * The session store rather than `useSessionUser()`: the route context is the
 * user as the guard read it, and `updatePreferences` does not refresh it.
 */
function storedClock(): boolean | null {
  const current = session.peek();
  return current?.state === 'authenticated'
    ? (current.user.clock_24h ?? null)
    : null;
}

/**
 * 13:00 on 1 January 2000, read in UTC: "13" on a 24-hour clock, "1 PM" on a
 * 12-hour one. In January so no zone's summer time can move it.
 */
const ONE_PM = Date.UTC(2000, 0, 1, 13);

/**
 * The 24-hour clock switch, Sentry's "Use a 24-hour clock".
 *
 * `users.clock_24h` is three states and the switch shows two. Until the reader
 * flips it, the column is `null` and the switch shows the clock their language
 * already uses: on in French, off in English. Flipping it stores the choice,
 * and from then on it holds whatever language they read in.
 *
 * Written to the account, like the language and the zone, so it follows the
 * reader to any browser. `intl.reload()` applies it: only the formatters read
 * it, and they read it from the snapshot the reload republishes.
 */
export function ClockSelector() {
  const t = useTranslations('locale');
  const format = useFormatter();
  const stored = useSyncExternalStore(session.subscribe, storedClock);
  // Pending until the formatters have the new clock, not only until it is saved.
  const save = useMutation({
    ...userMutations.updatePreferences(),
    onSuccess: () => intl.reload(),
  });
  const fieldId = useId();

  // The language's own clock, read off the formatter that renders every
  // date, so the switch cannot disagree with what the pages show. Explicit
  // options, not a named format: those carry the stored clock, and this is
  // what the language does without one.
  const languageClock24h = format
    .dateTime(ONE_PM, { hour: 'numeric', timeZone: 'UTC' })
    .includes('13');
  const checked = stored ?? languageClock24h;

  const switchTo = (next: boolean) => {
    save.mutate(
      { clock_24h: next },
      { onError: () => toast.error(t('clockSaveFailed')) },
    );
  };

  return (
    <SettingRow
      title={t('clockLabel')}
      description={t('clockHint')}
      htmlFor={fieldId}
    >
      <Switch
        id={fieldId}
        checked={checked}
        onCheckedChange={switchTo}
        // The save is a server round trip. Without the pending state a slow
        // response reads as a dead control and invites a second flip.
        disabled={save.isPending}
      />
    </SettingRow>
  );
}
