import { Combobox } from '@base-ui/react/combobox';
import { ChevronDownIcon, SelectedIcon } from '@rustrak/ui';
import { PlatformIcon } from 'platformicons';
import { useMemo } from 'react';
import { useTranslations } from 'use-intl';
import {
  languageLabel,
  PLATFORMS,
  type Platform,
} from '@/shared/config/platforms';
import { cn } from '@/shared/lib/utils';

interface PlatformPickerProps {
  value: string | null;
  onValueChange: (platform: string) => void;
  disabled?: boolean;
}

/**
 * Searchable platform picker. A plain select is unusable at this size: there
 * are ~120 platforms across ~39 language groups, so typing to filter is the
 * only practical way in, matching how Sentry's own picker behaves.
 */
export function PlatformPicker({
  value,
  onValueChange,
  disabled,
}: PlatformPickerProps) {
  const t = useTranslations('platforms');
  // Group by language so frameworks sit under the language they belong to,
  // which is what makes ~120 options navigable when not filtering.
  const groups = useMemo(() => {
    const byLanguage = new Map<string, Platform[]>();
    for (const platform of PLATFORMS) {
      const existing = byLanguage.get(platform.language);
      if (existing) {
        existing.push(platform);
      } else {
        byLanguage.set(platform.language, [platform]);
      }
    }
    return [...byLanguage.entries()]
      .map(([language, items]) => ({ value: languageLabel(language), items }))
      .sort((a, b) => a.value.localeCompare(b.value));
  }, []);

  const selected = useMemo(
    () => PLATFORMS.find((p) => p.id === value) ?? null,
    [value],
  );

  return (
    <Combobox.Root
      items={groups}
      value={selected}
      onValueChange={(platform: Platform | null) => {
        if (platform) {
          onValueChange(platform.id);
        }
      }}
      itemToStringLabel={(platform: Platform) => platform.name}
      disabled={disabled}
    >
      <div className="relative w-full">
        <Combobox.Input
          placeholder={t('pickerPlaceholder')}
          aria-label={t('pickerAria')}
          className="flex h-9 w-full items-center rounded-md border border-border-field bg-transparent py-2 pr-8 pl-2.5 text-sm shadow-xs transition-[color,box-shadow] outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-50 dark:bg-surface-hover/30"
        />
        <Combobox.Icon
          render={
            <ChevronDownIcon className="pointer-events-none absolute top-2.5 right-2 size-4 text-fg-subtle" />
          }
        />
      </div>

      <Combobox.Portal>
        <Combobox.Positioner className="isolate z-50" sideOffset={4}>
          <Combobox.Popup className="max-h-72 w-(--anchor-width) origin-(--transform-origin) overflow-y-auto rounded-md bg-surface-floating p-1 text-fg shadow-md ring-1 ring-fg/10">
            <Combobox.Empty className="px-2 py-3 text-center text-sm text-fg-subtle">
              {t('noResults')}
            </Combobox.Empty>
            <Combobox.List>
              {(group: { value: string; items: Platform[] }) => (
                <Combobox.Group
                  key={group.value}
                  items={group.items}
                  className="scroll-my-1"
                >
                  <Combobox.GroupLabel className="px-2 py-1.5 text-xs text-fg-subtle">
                    {group.value}
                  </Combobox.GroupLabel>
                  <Combobox.Collection>
                    {(platform: Platform) => (
                      <Combobox.Item
                        key={platform.id}
                        value={platform}
                        className={cn(
                          'relative flex w-full cursor-default items-center gap-2 rounded-sm py-1.5 pr-8 pl-2 text-sm outline-hidden select-none',
                          'data-highlighted:bg-surface-hover data-highlighted:text-fg',
                        )}
                      >
                        <PlatformIcon platform={platform.id} size={16} />
                        <span className="flex-1 truncate">{platform.name}</span>
                        <Combobox.ItemIndicator className="absolute right-2">
                          <SelectedIcon className="size-4" />
                        </Combobox.ItemIndicator>
                      </Combobox.Item>
                    )}
                  </Combobox.Collection>
                </Combobox.Group>
              )}
            </Combobox.List>
          </Combobox.Popup>
        </Combobox.Positioner>
      </Combobox.Portal>
    </Combobox.Root>
  );
}
