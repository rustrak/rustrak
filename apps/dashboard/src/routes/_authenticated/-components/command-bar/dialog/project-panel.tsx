import { PlatformIcon } from 'platformicons';
import { useEffect, useRef } from 'react';
import { useTranslations } from 'use-intl';
import { cn } from '@/shared/lib/utils';
import type { CommandLink, CommandProject } from '../commands';

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
  pages,
  focusIndex,
  onFocusIndexChange,
  onNavigate,
  onToggle,
}: {
  project: CommandProject;
  pages: CommandLink[];
  /** Which page holds DOM focus. The column only exists while it is set. */
  focusIndex: number | null;
  onFocusIndexChange: (index: number) => void;
  onNavigate: (href: string) => void;
  /** Tab or Shift+Tab, which closes the column and returns focus to the list. */
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
    if (focusIndex === null || pages.length === 0) return;

    const step =
      event.key === 'ArrowDown' ? 1 : event.key === 'ArrowUp' ? -1 : 0;

    if (step !== 0) {
      event.preventDefault();
      event.stopPropagation();
      onFocusIndexChange((focusIndex + step + pages.length) % pages.length);
    } else if (event.key === 'Enter') {
      event.preventDefault();
      event.stopPropagation();
      // In range: the guard above and the modulo keep it inside `pages`.
      onNavigate(pages[focusIndex].href);
    } else if (event.key === 'Tab') {
      // Either direction leaves the column, so the input's keys go back to
      // the list rather than to a column that no longer has focus.
      event.preventDefault();
      event.stopPropagation();
      onToggle();
    }
  };

  return (
    <div
      onKeyDown={handleKeyDown}
      className={cn(
        'hidden w-[15rem] shrink-0 flex-col overflow-hidden p-1.5 md:flex',
        // A step down from the floating surface, so the column reads as a
        // pane beside the list rather than more of it.
        'border-border-divider border-l bg-surface',
      )}
    >
      <div className="flex shrink-0 items-center gap-2.5 px-2.5 pt-2.5 pb-3">
        <PlatformIcon
          platform={project.platform ?? 'other'}
          size={28}
          format="lg"
          className="shrink-0 rounded-md"
        />
        <div className="flex min-w-0 flex-col gap-0.5">
          <p className="truncate text-card-title text-fg">{project.name}</p>
          <p className="truncate font-mono text-fg-subtle text-mono-sm">
            {project.platform ?? t('commands.noPlatformSet')}
          </p>
        </div>
      </div>

      <p className="shrink-0 px-2.5 pt-2 pb-1.5 text-column text-fg-meta">
        {pages.length} {t('commands.pages')}
      </p>

      {/* Only the page list scrolls; the identity above it stays put. */}
      <ul className="flex min-h-0 flex-1 flex-col overflow-y-auto">
        {pages.map((page, index) => (
          <li key={page.href}>
            <button
              type="button"
              ref={(node) => {
                itemRefs.current[index] = node;
              }}
              // Roving focus: the column is entered deliberately with Tab and
              // walked with the arrows, so only the active page is a tab stop.
              tabIndex={focusIndex === index ? 0 : -1}
              onClick={() => onNavigate(page.href)}
              className="group/page flex h-8 w-full items-center gap-2.5 rounded-md px-2.5 text-left text-control text-fg-muted outline-none hover:bg-surface-hover hover:text-fg focus-visible:bg-surface-selected focus-visible:text-fg"
            >
              <page.icon className="size-icon-md shrink-0 text-fg-ghost group-hover/page:text-fg-subtle group-focus-visible/page:text-fg-subtle" />
              <span className="truncate">{t(page.labelKey)}</span>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
