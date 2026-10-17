import { Combobox } from '@base-ui/react/combobox';
import { CheckIcon, ChevronDownIcon } from 'lucide-react';
import { useMemo } from 'react';
import {
  listTimeZones,
  matchesTimeZone,
  type TimeZoneOption,
} from '@/shared/lib/time-zones';
import { cn } from '@/shared/lib/utils';

/**
 * A searchable IANA zone picker.
 *
 * Searchable rather than a plain `Select`: there are ~420 zones, and typing
 * `mad` or `+02` is the only practical way in. Each row carries its current
 * UTC offset so a reader can scan for their offset without knowing the IANA
 * name for it.
 */
export function TimeZoneCombobox({
  id,
  value,
  onChange,
  placeholder,
  noResults,
  disabled,
}: {
  id?: string;
  /** `undefined` shows the placeholder. */
  value: string | undefined;
  onChange: (zone: string | null) => void;
  placeholder: string;
  noResults: string;
  disabled?: boolean;
}) {
  // Offsets shift with daylight saving time, so the list is built for "now"
  // once per mount rather than at module load.
  const options = useMemo(() => listTimeZones(), []);
  const selected = useMemo(
    () => options.find((option) => option.value === value) ?? null,
    [options, value],
  );

  return (
    <Combobox.Root
      items={options}
      value={selected}
      onValueChange={(next: TimeZoneOption | null) =>
        onChange(next?.value ?? null)
      }
      itemToStringLabel={(option: TimeZoneOption) => option.value}
      filter={matchesTimeZone}
      disabled={disabled}
    >
      <div className="relative w-full">
        <Combobox.Input
          id={id}
          placeholder={placeholder}
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
              {noResults}
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
  );
}
