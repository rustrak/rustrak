import { PlatformIcon } from 'platformicons';
import { useEffect, useRef } from 'react';
import { useTranslations } from 'use-intl';
import {
  ALL_PROJECT_PAGES,
  type CommandProject,
  PROJECT_PAGE_COUNT,
} from '@/shared/config/commands';
import { cn } from '@/shared/lib/utils';
import { IconTile } from '../primitives/icon-tile';
import type { CommandTarget } from '../results/command-target';

/**
 * The preview column: the pages of whichever project is selected.
 *
 * It is a real focus region, not a passive readout. Tab moves into it, the
 * arrows walk it, Enter opens a page and Tab closes it again. That is the
 * whole reason the palette no longer needs a mode for "inside a project": the
 * pages are one keystroke away from the project row instead of behind a state
 * change that swapped the entire list.
 */
export function ProjectPanel({
  project,
  focusIndex,
  onFocusIndexChange,
  onNavigate,
  onToggle,
}: {
  project: CommandProject;
  /** Which page holds DOM focus. The column only exists while it is set. */
  focusIndex: number | null;
  onFocusIndexChange: (index: number) => void;
  onNavigate: (target: CommandTarget) => void;
  /** Tab, which closes the column and returns focus to the list. */
  onToggle: () => void;
}) {
  const t = useTranslations();
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);

  // DOM focus is state this component does not own, so it is synced rather
  // than set in the handler: entering the column happens in the *input's*
  // keydown, one component up, and there is no element to focus until this
  // one has rendered.
  useEffect(() => {
    if (focusIndex !== null) itemRefs.current[focusIndex]?.focus();
  }, [focusIndex]);

  // Every branch stops the event as well as preventing it. This column lives
  // inside `Command`, whose root handles the same keys for the list below: an
  // unstopped Enter reaches it and opens whatever row the *list* had selected,
  // and an unstopped arrow moves that list while the column walks its pages.
  const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (focusIndex === null) return;

    const step =
      event.key === 'ArrowDown' ? 1 : event.key === 'ArrowUp' ? -1 : 0;

    if (step !== 0) {
      event.preventDefault();
      event.stopPropagation();
      onFocusIndexChange(
        (focusIndex + step + ALL_PROJECT_PAGES.length) %
          ALL_PROJECT_PAGES.length,
      );
    } else if (event.key === 'Enter') {
      event.preventDefault();
      event.stopPropagation();
      onNavigate({
        to: ALL_PROJECT_PAGES[focusIndex].to,
        params: { id: project.id },
      });
    } else if (event.key === 'Tab' && !event.shiftKey) {
      event.preventDefault();
      event.stopPropagation();
      onToggle();
    }
  };

  return (
    <div
      onKeyDown={handleKeyDown}
      className={cn(
        'hidden w-[18rem] shrink-0 flex-col overflow-hidden p-5 md:flex',
        // `--border` is four points of luminance off the popover here, so the
        // column is separated by its own surface, not by an invisible line.
        'border-l border-foreground/10 bg-foreground/[0.03]',
      )}
    >
      <div className="flex shrink-0 flex-col items-center gap-3 text-center">
        <IconTile size="detail">
          <PlatformIcon
            platform={project.platform ?? 'other'}
            size={32}
            format="lg"
            className="rounded-md"
          />
        </IconTile>
        <div className="flex flex-col gap-1">
          <p className="text-base leading-tight font-semibold">
            {project.name}
          </p>
          <p className="text-xs text-muted-foreground">
            {project.platform ?? t('commands.noPlatformSet')}
          </p>
        </div>
      </div>

      <div className="my-5 h-px shrink-0 bg-foreground/10" />

      <p className="mb-2 shrink-0 text-[11px] font-semibold tracking-[0.09em] text-muted-foreground/60 uppercase">
        {PROJECT_PAGE_COUNT} {t('commands.pages')}
      </p>

      {/* Only the page list scrolls; the identity above it stays put. */}
      <ul className="-mx-2 flex min-h-0 flex-1 flex-col overflow-y-auto px-2">
        {ALL_PROJECT_PAGES.map((page, index) => (
          <li key={page.to}>
            <button
              type="button"
              ref={(node) => {
                itemRefs.current[index] = node;
              }}
              // Roving focus: the column is entered deliberately with Tab and
              // walked with the arrows, so only the active page is a tab stop.
              tabIndex={focusIndex === index ? 0 : -1}
              onClick={() =>
                onNavigate({ to: page.to, params: { id: project.id } })
              }
              className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm text-muted-foreground transition-colors duration-100 outline-none hover:bg-primary/10 hover:text-primary focus-visible:bg-primary/10 focus-visible:text-primary"
            >
              <page.icon className="size-3.5 shrink-0 opacity-70" />
              <span className="truncate">{t(page.labelKey)}</span>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
