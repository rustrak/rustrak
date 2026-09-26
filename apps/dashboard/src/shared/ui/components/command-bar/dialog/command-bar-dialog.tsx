import { useNavigate } from '@tanstack/react-router';
import { useMemo, useRef, useState } from 'react';
import { useTranslations } from 'use-intl';
import {
  ALL_PROJECT_PAGES,
  type CommandProject,
  PROJECT_COMMANDS,
} from '@/shared/config/commands';
import { filterCommand } from '@/shared/lib/command-score';
import { cn } from '@/shared/lib/utils';
import { Command, CommandInput } from '@/shared/ui/components/shadcn/command';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '@/shared/ui/components/shadcn/dialog';
import { useIsMobile } from '@/shared/ui/hooks/use-mobile';
import type { CommandTarget } from '../results/command-target';
import { ResultsList } from '../results/results-list';
import { CommandBarFooter } from './footer';
import { ProjectPanel } from './project-panel';

/**
 * cmdk lowercases item values and collapses their whitespace before it reports
 * the selected one, so matching a row back to its project has to normalise the
 * same way or every selection misses.
 */
const rowKey = (value: string) =>
  value.toLowerCase().replace(/\s+/g, ' ').trim();

interface CommandBarDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  projects: CommandProject[];
}

export function CommandBarDialog({
  open,
  onOpenChange,
  projects,
}: CommandBarDialogProps) {
  const t = useTranslations();
  const navigate = useNavigate();
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  // The preview column is `md:flex`, and 768px is exactly that breakpoint.
  // Below it the column is `display: none`, where `focus()` is a no-op, so
  // every key the column claims would land nowhere.
  const isMobile = useIsMobile();
  const [query, setQuery] = useState('');
  /**
   * Which page of the preview column has focus, and by the same token whether
   * the column is showing at all. There is no state for "open but not focused"
   * because Tab is the only way in or out and it always lands inside.
   */
  const [panelIndex, setPanelIndex] = useState<number | null>(null);
  const panelIndexRef = useRef<number | null>(null);

  // Seeded with the top row rather than empty. cmdk is controlled here, and a
  // controlled value matching no row leaves *nothing* highlighted instead of
  // falling back to the first one, so an empty initial value would open the
  // palette with a dead list.
  const [selected, setSelected] = useState(
    () => projects[0]?.name ?? t(PROJECT_COMMANDS[0].labelKey),
  );

  // Only a project row can be previewed, and its row value is its name.
  const selectedProject = useMemo(() => {
    const key = rowKey(selected);
    return projects.find((project) => rowKey(project.name) === key) ?? null;
  }, [projects, selected]);

  // Derived rather than stored, so a selection that stops being a project --
  // or a viewport that drops below `md` mid-session -- takes the column with
  // it without an effect chasing it.
  const canPreview = selectedProject !== null && !isMobile;
  const showPreview = canPreview && panelIndex !== null;

  // Nothing to tear down on the way out: `CommandBar` keys this component on
  // the open count, so the next open is a new instance and every piece of
  // state below starts from its initial value again.
  const go = (target: CommandTarget) => {
    onOpenChange(false);
    navigate(target);
  };

  /**
   * A keystroke replaces the result set, and cmdk will not put the list back
   * at the top for us: it only ever scrolls the selected row into view, and
   * that is a no-op whenever the row it settles on already sits inside the
   * visible box. So a query typed against a scrolled list keeps the old
   * offset and opens on rows halfway down.
   *
   * Reset from here rather than an effect. The scroll is set while the old
   * rows are still on screen, before React renders the new ones, so they only
   * ever paint at the top -- an effect runs after the paint and would show one
   * frame at the stale offset.
   */
  const search = (value: string) => {
    if (listRef.current) listRef.current.scrollTop = 0;
    setQuery(value);
  };

  // Hovering a row moves cmdk's selection, unless the column has focus: the
  // pointer resting over the list while the arrows walk the pages would
  // otherwise yank the selection back and swap the column out from under it.
  const highlight = (value: string) => {
    if (panelIndex === null) setSelected(value);
  };

  /**
   * Mirrored into a ref because the keyboard reads it back in the same task it
   * writes it: two keys pressed inside one frame both run against the render
   * that preceded them, and a handler reading `panelIndex` from its closure
   * would still see the value from before the previous keystroke. State drives
   * the rendering, the ref answers "where am I now".
   */
  const movePanel = (index: number | null) => {
    panelIndexRef.current = index;
    setPanelIndex(index);
  };

  // Tab is the only way in and out of the column, so it is also the only thing
  // that decides whether the column exists.
  const togglePanel = () => {
    if (panelIndexRef.current !== null) {
      movePanel(null);
      inputRef.current?.focus();
    } else {
      movePanel(0);
    }
  };

  /**
   * Keys while the page column holds the selection.
   *
   * cmdk listens on its own root above this input, so anything claimed here
   * has to stop bubbling or the list would move as well.
   */
  const handlePanelKey = (
    event: React.KeyboardEvent<HTMLInputElement>,
    current: number,
    projectId: number,
  ) => {
    const step =
      event.key === 'ArrowDown' ? 1 : event.key === 'ArrowUp' ? -1 : 0;

    if (step !== 0) {
      event.preventDefault();
      event.stopPropagation();
      movePanel(
        (current + step + ALL_PROJECT_PAGES.length) % ALL_PROJECT_PAGES.length,
      );
      return;
    }

    if (event.key === 'Tab' && !event.shiftKey) {
      event.preventDefault();
      togglePanel();
      return;
    }

    if (event.key === 'Enter') {
      event.preventDefault();
      event.stopPropagation();
      go({ to: ALL_PROJECT_PAGES[current].to, params: { id: projectId } });
    }
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    // `isMobile` as well as the selection: below `md` there is no column to
    // enter, and claiming Tab there would strand `panelIndexRef` non-null and
    // route every subsequent arrow and Enter into a hidden subtree, which
    // reads as the visible list having died.
    if (!selectedProject || isMobile) return;

    // DOM focus reaches the column one render after `panelIndex` is set, so a
    // quick second keystroke still arrives here. Routing by intent rather than
    // by where focus happens to sit means entering the column never eats the
    // key that follows it.
    const current = panelIndexRef.current;
    if (current !== null) {
      handlePanelKey(event, current, selectedProject.id);
      return;
    }

    // Safe to take: a project row is only ever selected with an empty query,
    // so no caret is being moved, and the input is the palette's single tab
    // stop. Shift+Tab is left alone.
    if (event.key === 'Tab' && !event.shiftKey) {
      event.preventDefault();
      togglePanel();
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="top-[10vh] flex max-h-[80svh] w-[calc(100%-2rem)] translate-y-0 flex-col gap-0 overflow-hidden rounded-xl! p-0 sm:max-w-[52rem]"
        showCloseButton={false}
      >
        <DialogTitle className="sr-only">
          {t('commands.dialogTitle')}
        </DialogTitle>
        <DialogDescription className="sr-only">
          {t('commands.dialogDescription')}
        </DialogDescription>

        <Command
          loop
          filter={filterCommand}
          value={selected}
          onValueChange={setSelected}
          className="min-h-0 flex-1 p-0"
        >
          <div
            className={cn(
              'flex shrink-0 items-center gap-2 p-3',
              '*:data-[slot=command-input-wrapper]:min-w-0',
              '*:data-[slot=command-input-wrapper]:flex-1',
              '*:data-[slot=command-input-wrapper]:p-0',
            )}
          >
            <CommandInput
              ref={inputRef}
              // Base UI parks focus on the dialog popup itself, and cmdk
              // listens for arrow keys on its own root below that, so without
              // this the palette opens with a dead keyboard. Enough on its own
              // now that every open is a fresh mount.
              autoFocus
              value={query}
              onValueChange={search}
              onKeyDown={handleKeyDown}
              className="h-10! text-[15px]!"
              placeholder={t('commands.searchPlaceholder')}
            />
          </div>

          {/* Fixed height so the list does not resize under the selection as
              it shortens with the query. It gives way only when the dialog
              hits its own cap, which is the short-viewport case. */}
          <div className="flex h-[30rem] min-h-0 border-t border-foreground/10">
            <ResultsList
              ref={listRef}
              projects={projects}
              query={query}
              onNavigate={go}
              onHighlight={highlight}
            />

            {showPreview && selectedProject ? (
              <ProjectPanel
                project={selectedProject}
                focusIndex={panelIndex}
                onFocusIndexChange={movePanel}
                onNavigate={go}
                onToggle={togglePanel}
              />
            ) : null}
          </div>

          {/* `canPreview`, so the hint is not advertising a Tab that does
              nothing on the viewports where the column cannot open. */}
          <CommandBarFooter hasPreview={canPreview} previewOpen={showPreview} />
        </Command>
      </DialogContent>
    </Dialog>
  );
}
