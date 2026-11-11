import { Dialog as BaseDialog } from '@base-ui/react/dialog';
import { Dialog, Kbd, SearchIcon, useMobileBreakpoint } from '@rustrak/ui';
import { Command as CommandPrimitive } from 'cmdk';
import { useMemo, useRef, useState } from 'react';
import { useTranslations } from 'use-intl';
import { usePermissionsLater } from '@/shared/ui/hooks/use-permissions';
import { useRouter } from '@/shared/ui/hooks/use-router';
import { filterCommand } from '../command-score';
import {
  type CommandProject,
  commandRow,
  projectCommands,
  projectRow,
  rowProject,
} from '../commands';
import { Command } from '../primitives/command';
import { ResultsList } from '../results/results-list';
import { CommandBarFooter } from './footer';
import { ProjectPanel } from './project-panel';

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
  const router = useRouter();
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const permissions = usePermissionsLater();
  // The preview column is `md:flex`, the same breakpoint. Below it the column
  // is `display: none`, where `focus()` is a no-op, so every key the column
  // claims would land nowhere.
  const isMobile = useMobileBreakpoint().mobile;
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
  const [selected, setSelected] = useState(() =>
    projects[0] ? projectRow(projects[0].id) : commandRow('/projects'),
  );

  // Only a project row can be previewed, resolved by id: names are not unique.
  const selectedProject = rowProject(projects, selected);
  const pages = useMemo(
    () =>
      selectedProject ? projectCommands(selectedProject, permissions) : [],
    [selectedProject, permissions],
  );

  // Derived rather than stored, so a selection that stops being a project --
  // or a viewport that drops below `md` mid-session -- takes the column with
  // it without an effect chasing it. A project with no page the reader may
  // open has nothing to preview.
  const canPreview = pages.length > 0 && !isMobile;
  const showPreview = canPreview && panelIndex !== null;

  // Nothing to tear down on the way out: `CommandBar` keys this component on
  // the open count, so the next open is a new instance and every piece of
  // state below starts from its initial value again.
  const go = (href: string) => {
    onOpenChange(false);
    router.push(href);
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
  ) => {
    const step =
      event.key === 'ArrowDown' ? 1 : event.key === 'ArrowUp' ? -1 : 0;

    if (step !== 0) {
      event.preventDefault();
      event.stopPropagation();
      movePanel((current + step + pages.length) % pages.length);
      return;
    }

    // Either direction closes the column, as it does from inside it.
    if (event.key === 'Tab') {
      event.preventDefault();
      togglePanel();
      return;
    }

    if (event.key === 'Enter') {
      event.preventDefault();
      event.stopPropagation();
      const page = pages[current];
      if (page) go(page.href);
    }
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    // `isMobile` as well as the selection: below `md` there is no column to
    // enter, and claiming Tab there would strand `panelIndexRef` non-null and
    // route every subsequent arrow and Enter into a hidden subtree, which
    // reads as the visible list having died.
    // `canPreview` also means `pages` is not empty, so nothing below divides
    // by zero or reads a page that is not there.
    if (!canPreview) return;

    // DOM focus reaches the column one render after `panelIndex` is set, so a
    // quick second keystroke still arrives here. Routing by intent rather than
    // by where focus happens to sit means entering the column never eats the
    // key that follows it.
    const current = panelIndexRef.current;
    if (current !== null) {
      handlePanelKey(event, current);
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
    // `xl` only lifts the size cap; the palette's own width and its place in
    // the top third of the screen come from the classes.
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      size="xl"
      className="mt-[calc(14vh-1.5rem)] max-h-[76svh] self-start rounded-2xl bg-surface-floating text-fg shadow-overlay sm:max-w-[42rem]"
    >
      {/* Named for the screen reader only: the search field is the palette's
          visible heading, so `DialogHeader` would repeat it. */}
      <BaseDialog.Title className="sr-only">
        {t('commands.dialogTitle')}
      </BaseDialog.Title>
      <BaseDialog.Description className="sr-only">
        {t('commands.dialogDescription')}
      </BaseDialog.Description>

      <Command
        loop
        filter={filterCommand}
        value={selected}
        onValueChange={setSelected}
        className="flex-1"
      >
        {/* The field is the palette's top edge rather than a box inside it:
              no frame of its own, a rule under it, the way the design
              system's search fields sit in their surface. */}
        <div className="flex h-14 shrink-0 items-center gap-3 border-border-divider border-b px-4">
          <SearchIcon size="lg" className="text-fg-ghost" />
          <CommandPrimitive.Input
            ref={inputRef}
            // Base UI parks focus on the dialog popup itself, and cmdk
            // listens for arrow keys on its own root below that, so without
            // this the palette opens with a dead keyboard. Enough on its own
            // now that every open is a fresh mount.
            autoFocus
            value={query}
            onValueChange={search}
            onKeyDown={handleKeyDown}
            className="h-full min-w-0 flex-1 bg-transparent text-fg text-section font-normal tracking-normal outline-none placeholder:text-fg-placeholder"
            placeholder={t('commands.searchPlaceholder')}
          />
          <Kbd>esc</Kbd>
        </div>

        {/* Fixed height so the list does not resize under the selection as
              it shortens with the query. It gives way only when the dialog
              hits its own cap, which is the short-viewport case. */}
        <div className="flex h-[26rem] min-h-0">
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
              pages={pages}
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
    </Dialog>
  );
}
