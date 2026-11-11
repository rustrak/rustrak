import { type ReactNode, useEffect, useState } from 'react';
import type { CommandProject } from './commands';
import { CommandBarDialog } from './dialog/command-bar-dialog';

const IS_MAC = /mac|iphone|ipad|ipod/i.test(navigator.userAgent);

interface CommandBarProps {
  projects: CommandProject[];
  /**
   * Draws what opens the bar, given the opener and the shortcut label for this
   * machine. The bar does not own it: it lives wherever the frame puts search.
   */
  trigger: (open: () => void, shortcut: string) => ReactNode;
}

export function CommandBar({ projects, trigger }: CommandBarProps) {
  const [open, setOpen] = useState(false);
  /**
   * Bumped on every open, and used as the dialog's `key`, so each open gets a
   * brand new component instance.
   *
   * This is the reset. The query, the highlighted row, whether the preview
   * column is showing and where the list is scrolled are all just initial
   * state on a fresh mount, so none of them need clearing by hand on the way
   * out. It is bumped when *opening* rather than derived from `open`, because
   * keying on `open` would tear the dialog out mid close animation.
   */
  const [session, setSession] = useState(0);

  const openBar = () => {
    setSession((count) => count + 1);
    setOpen(true);
  };

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      // `event.repeat` guards the hold: autorepeat keeps firing after the open
      // has committed, and the branch below would read the new `open` and shut
      // the palette again, so holding the shortcut flickers it.
      if (event.repeat) return;
      if (event.key !== 'k' || !(event.metaKey || event.ctrlKey)) return;
      event.preventDefault();
      if (open) setOpen(false);
      else openBar();
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [open]);

  const shortcut = IS_MAC ? '⌘K' : 'Ctrl K';

  return (
    <>
      {/* The handler takes either modifier, so the hint has to name the one
          this machine actually has rather than always showing ⌘. */}
      {trigger(openBar, shortcut)}

      <CommandBarDialog
        key={session}
        open={open}
        onOpenChange={setOpen}
        projects={projects}
      />
    </>
  );
}
