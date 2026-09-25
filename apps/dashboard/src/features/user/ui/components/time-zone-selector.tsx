import { Combobox } from '@base-ui/react/combobox';
import { CheckIcon, ChevronDownIcon } from 'lucide-react';
import { useId, useMemo, useTransition } from 'react';
import { toast } from 'sonner';
import { useTimeZone, useTranslations } from 'use-intl';
import { updatePreferences } from '@/features/user/api/mutations';
import {
  listTimeZones,
  matchesTimeZone,
  type TimeZoneOption,
} from '@/features/user/lib/time-zones';
import { intl } from '@/shared/i18n/intl';
import { cn } from '@/shared/lib/utils';
import { Label } from '@/shared/ui/components/shadcn/label';

/**
 * The timezone control.
 *
 * `TimeZoneSync` already adopts the browser's zone onto the account the first
 * time a reader arrives without one, which covers almost everyone. What it
 * cannot do is let a reader *disagree* with their browser, and the one
 * disagreement that matters is "show me UTC on purpose": the zone events
 * arrive in and the one server logs are written in, so someone correlating a
 * stack trace against a log line wants both clocks to read the same. Sentry
 * stores the same preference as `user.options.timezone` and offers it as a
 * select on the account page; this is that select.
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

  // Offsets shift with daylight saving time, so the list is built for "now"
  // once per mount rather than at module load.
  const options = useMemo(() => listTimeZones(), []);
  const selected = useMemo(
    () => options.find((option) => option.value === timeZone) ?? null,
    [options, timeZone],
  );

  const switchTo = (next: TimeZoneOption | null) => {
    if (!next || next.value === timeZone) return;

    startTransition(async () => {
      const result = await updatePreferences({ timezone: next.value });
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
      <Combobox.Root
        items={options}
        value={selected}
        onValueChange={switchTo}
        itemToStringLabel={(option: TimeZoneOption) => option.value}
        filter={matchesTimeZone}
        // The save is a server round trip. Without the pending state a slow
        // response reads as a dead control and invites a second pick.
        disabled={isPending}
      >
        <div className="relative w-full">
          <Combobox.Input
            id={fieldId}
            placeholder={t('timeZonePlaceholder')}
            className="flex h-9 w-full items-center rounded-md border border-input bg-transparent py-2 pr-8 pl-2.5 text-sm shadow-xs transition-[color,box-shadow] outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-50 dark:bg-input/30"
          />
          <Combobox.Icon
            render={
              <ChevronDownIcon className="pointer-events-none absolute top-2.5 right-2 size-4 text-muted-foreground" />
            }
          />
        </div>

        <Combobox.Portal>
          <Combobox.Positioner className="isolate z-50" sideOffset={4}>
            <Combobox.Popup className="max-h-72 w-(--anchor-width) origin-(--transform-origin) overflow-y-auto rounded-md bg-popover p-1 text-popover-foreground shadow-md ring-1 ring-foreground/10">
              {/* `empty:p-0`, not `hidden`: Base UI keeps this mounted as the live
                  region that announces "no results", and its padding would
                  otherwise sit as a blank strip above every non-empty list. */}
              <Combobox.Empty className="px-2 py-3 text-center text-sm text-muted-foreground empty:p-0">
                {t('timeZoneNoResults')}
              </Combobox.Empty>
              <Combobox.List>
                {(option: TimeZoneOption) => (
                  <Combobox.Item
                    key={option.value}
                    value={option}
                    className={cn(
                      'relative flex w-full cursor-default items-center gap-2 rounded-sm py-1.5 pr-8 pl-2 text-sm outline-hidden select-none',
                      'data-highlighted:bg-accent data-highlighted:text-accent-foreground',
                    )}
                  >
                    <span className="flex-1 truncate">{option.value}</span>
                    <span className="shrink-0 font-mono text-xs text-muted-foreground">
                      {option.offset}
                    </span>
                    <Combobox.ItemIndicator className="absolute right-2">
                      <CheckIcon className="size-4" />
                    </Combobox.ItemIndicator>
                  </Combobox.Item>
                )}
              </Combobox.List>
            </Combobox.Popup>
          </Combobox.Positioner>
        </Combobox.Portal>
      </Combobox.Root>
      <p className="text-xs text-muted-foreground">{t('timeZoneHint')}</p>
    </div>
  );
}
