import { useId, useTransition } from 'react';
import { toast } from 'sonner';
import { useTimeZone, useTranslations } from 'use-intl';
import { updatePreferences } from '@/features/user/api/mutations';
import { session } from '@/shared/api/session';
import { intl } from '@/shared/i18n/intl';
import { Label } from '@/shared/ui/components/shadcn/label';
import { TimeZoneCombobox } from '@/shared/ui/components/time-zone-combobox';

/** The zone on the account row right now, or `null` when it holds none. */
function storedTimeZone(): string | null {
  const current = session.peek();
  return current?.state === 'authenticated'
    ? (current.user.timezone ?? null)
    : null;
}

/**
 * The timezone control.
 *
 * `TimeZoneSync` already adopts the browser's zone onto the account the first
 * time a reader arrives without one, which covers almost everyone. What it
 * cannot do is let a reader *disagree* with their browser, and the one
 * disagreement that matters is "show me UTC on purpose": the zone events
 * record their timestamps in, and the one server logs use too unless
 * `RUSTRAK_LOG_TIMEZONE` moves them, so someone correlating a stack trace
 * against a log line wants both clocks to read the same. Sentry stores the
 * same preference as `user.options.timezone` and offers it as a select on
 * the account page; this is that select.
 *
 * Searchable rather than a plain `Select`, for the same reason the platform
 * picker is: there are ~420 zones, and typing `mad` or `+02` is the only
 * practical way in. Each row carries its current UTC offset so a reader can
 * scan for their offset without knowing the IANA name for it.
 *
 * The choice is written to `users.timezone`, so it follows the reader to any
 * browser they log in from, and `TimeZoneSync` never overwrites a stored zone.
 * `intl.reload()` rather than a route refresh: nothing a loader fetched
 * depends on the zone, only the formatters do, and they read it from the
 * snapshot the reload republishes.
 */
export function TimeZoneSelector() {
  const t = useTranslations('locale');
  const timeZone = useTimeZone();
  const [isPending, startTransition] = useTransition();
  const fieldId = useId();

  const switchTo = (next: string | null) => {
    // Compared against the zone the account *stores*, not the one the
    // dashboard *resolved*: an account with no zone (or one this runtime does
    // not know) resolves to UTC, and a reader picking UTC there is choosing
    // it on purpose, so the pick has to reach the row. `peek()` rather than
    // the route context, which `updatePreferences` does not refresh.
    if (!next || next === storedTimeZone()) return;

    startTransition(async () => {
      const result = await updatePreferences({ timezone: next });
      if (!result.success) {
        toast.error(t('timeZoneSaveFailed'));
        return;
      }
      await intl.reload();
    });
  };

  return (
    <div className="flex flex-col gap-2 sm:max-w-xs">
      <Label htmlFor={fieldId}>{t('timeZoneLabel')}</Label>
      <TimeZoneCombobox
        id={fieldId}
        value={timeZone}
        onChange={switchTo}
        placeholder={t('timeZonePlaceholder')}
        noResults={t('timeZoneNoResults')}
        // The save is a server round trip. Without the pending state a slow
        // response reads as a dead control and invites a second pick.
        disabled={isPending}
      />
      <p className="text-xs text-muted-foreground">{t('timeZoneHint')}</p>
    </div>
  );
}
