import {
  focusRing,
  Input,
  interactiveTransition,
  pressScale,
  SearchIcon,
  SelectedIcon,
} from '@rustrak/ui';
import { PlatformIcon } from 'platformicons';
import { useMemo, useState } from 'react';
import { useTranslations } from 'use-intl';
import {
  categoryPlatforms,
  PLATFORM_CATEGORIES,
  type Platform,
  searchPlatforms,
} from '@/shared/config/platforms';
import { cn } from '@/shared/lib/utils';

/** Sentry's generic escape hatch, a real platform id the server accepts. */
const OTHER_PLATFORM_ID = 'other';

interface PlatformGridProps {
  value: string | null;
  onValueChange: (platformId: string) => void;
  disabled?: boolean;
}

/**
 * Category tabs + search + icon grid, mirroring Sentry's own create-project
 * picker rather than the searchable combobox used in project settings. The
 * combobox is right for changing one field in a settings form; picking a
 * platform is the primary act of this page, so it gets the room.
 */
export function PlatformGrid({
  value,
  onValueChange,
  disabled,
}: PlatformGridProps) {
  const t = useTranslations('platforms');
  const [category, setCategory] = useState('popular');
  const [query, setQuery] = useState('');

  // A non-empty query deliberately searches every platform, ignoring the
  // active tab. Sentry does the same: someone who types "django" while Browser
  // is open should find it rather than be told there are no results.
  //
  // `other` is pulled out of the list and pinned as the first card in every
  // tab: it is the escape hatch, and burying it alphabetically under "O" in a
  // 127-entry grid hides the one option that always applies.
  const platforms: Platform[] = useMemo(
    () =>
      (query.trim()
        ? searchPlatforms(query)
        : categoryPlatforms(category)
      ).filter((p) => p.id !== OTHER_PLATFORM_ID),
    [query, category],
  );

  const isSearching = query.trim().length > 0;
  const otherSelected = value === OTHER_PLATFORM_ID;

  return (
    <div className="overflow-hidden rounded-2xl border border-border-subtle bg-surface">
      <div className="flex flex-col gap-3 border-b border-border-divider p-3 sm:flex-row-reverse sm:items-center">
        <Input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={t('searchPlaceholder')}
          aria-label={t('searchAria')}
          disabled={disabled}
          leading={<SearchIcon size="md" />}
          boxClassName="sm:w-60 sm:shrink-0"
        />

        <div
          className="-mx-1 flex flex-1 gap-1 overflow-x-auto px-1"
          role="tablist"
          aria-label={t('categoriesAria')}
        >
          {PLATFORM_CATEGORIES.map((c) => (
            <button
              key={c.id}
              type="button"
              role="tab"
              aria-selected={!isSearching && category === c.id}
              disabled={disabled}
              onClick={() => {
                setCategory(c.id);
                // Changing tab clears the query, otherwise the tab appears to
                // do nothing while a search is active.
                setQuery('');
              }}
              className={cn(
                'flex h-control-sm shrink-0 items-center rounded-md px-3',
                'text-control disabled:opacity-50',
                interactiveTransition,
                focusRing,
                !isSearching && category === c.id
                  ? 'bg-surface-selected text-fg'
                  : 'text-fg-muted hover:bg-surface-hover hover:text-fg',
              )}
            >
              {t(`categories.${c.id}`)}
            </button>
          ))}
        </div>
      </div>
      {platforms.length === 0 ? (
        // Sentry's own empty state points at "Other" rather than leaving the
        // user stuck, since `other` is a real, storable platform id and the
        // picker otherwise requires a choice.
        <div className="px-4 py-16 text-center">
          <p className="text-value font-medium text-fg">
            {t('noSdkFor', { query: query.trim() })}
          </p>
          <p className="mx-auto mt-1 max-w-md text-value text-fg-subtle">
            {t.rich('tryGeneric', {
              other: (chunks) => (
                <button
                  type="button"
                  disabled={disabled}
                  onClick={() => {
                    setQuery('');
                    onValueChange(OTHER_PLATFORM_ID);
                  }}
                  className="rounded-xs font-medium text-fg-brand underline-offset-4 hover:underline disabled:opacity-50"
                >
                  {chunks}
                </button>
              ),
            })}
          </p>
        </div>
      ) : (
        // The inner scroll area only earns its keep next to the sticky aside on
        // large screens. On a phone it nests a scroll inside the page scroll,
        // which fights the thumb; there the grid just flows, which is what
        // Sentry's own picker does at every width.
        <div className="overflow-visible p-3 lg:max-h-[28rem] lg:overflow-y-auto">
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-3 xl:grid-cols-4">
            {/* Full row rather than a tile: the copy has to be readable to do
                its job, and a grid cell truncates it. Spanning also stops it
                reading as "just another platform", which it is not. */}
            <button
              type="button"
              disabled={disabled}
              aria-pressed={otherSelected}
              onClick={() => onValueChange(OTHER_PLATFORM_ID)}
              className={cn(
                'col-span-full flex items-center gap-3 rounded-xl border border-dashed p-3 text-start',
                'disabled:opacity-50',
                interactiveTransition,
                focusRing,
                otherSelected
                  ? 'border-border-brand bg-surface-selected'
                  : 'border-border-strong hover:bg-surface-hover',
              )}
            >
              <PlatformIcon platform={OTHER_PLATFORM_ID} size={28} />
              <span className="min-w-0 flex-1">
                <span className="block text-value font-medium text-fg">
                  {t('otherPlatform')}
                </span>
                <span className="block text-hint text-fg-subtle">
                  {t('otherPlatformDescription')}
                </span>
              </span>
              {otherSelected && (
                <SelectedIcon size="lg" className="shrink-0 text-fg-brand" />
              )}
            </button>

            {platforms.map((p) => {
              const selected = value === p.id;
              return (
                <button
                  key={p.id}
                  type="button"
                  disabled={disabled}
                  aria-pressed={selected}
                  onClick={() => onValueChange(p.id)}
                  className={cn(
                    'flex items-center gap-3 rounded-xl border p-3 text-start',
                    'disabled:opacity-50',
                    interactiveTransition,
                    pressScale,
                    focusRing,
                    selected
                      ? 'border-border-brand bg-surface-selected'
                      : 'border-border-subtle hover:border-border-strong hover:bg-surface-hover',
                  )}
                >
                  <PlatformIcon
                    platform={p.id}
                    size={28}
                    className="shrink-0 rounded-md"
                  />
                  <span className="min-w-0 flex-1 truncate text-value font-medium text-fg">
                    {p.name}
                  </span>
                  {selected && (
                    <SelectedIcon
                      size="lg"
                      className="shrink-0 text-fg-brand"
                    />
                  )}
                </button>
              );
            })}
          </div>
        </div>
      )}
      <div className="border-t border-border-divider px-4 py-2 text-hint text-fg-meta">
        {isSearching
          ? t('matchingCount', { count: platforms.length })
          : t('footerCount', { count: platforms.length })}
      </div>
    </div>
  );
}
