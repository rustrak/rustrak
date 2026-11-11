import { useEffect, useLayoutEffect, useRef } from 'react';

/**
 * True while a key belongs to something else, when single-key shortcuts stand
 * down: a field being typed in, and a dialog or menu with focus -- an `r` in
 * the delete confirmation must not resolve the issue behind it, and a menu's
 * typeahead must not also run the page's action.
 */
function typing(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  return Boolean(
    element?.closest(
      'input, textarea, select, [contenteditable="true"], [role="dialog"], [role="alertdialog"], [role="menu"], [role="listbox"]',
    ),
  );
}

/**
 * Single-key shortcuts for the page, Linear-style: off while a field has
 * focus, and never with a modifier other than Shift, so the browser's own
 * shortcuts are untouched.
 */
export function useShortcuts(keys: Record<string, () => void>) {
  // The listener is added once and reads the latest keys through this ref,
  // written after each commit -- never during a render React may discard.
  const latest = useRef(keys);
  useLayoutEffect(() => {
    latest.current = keys;
  });

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      if (typing(event.target)) return;
      const action = latest.current[event.key];
      if (!action) return;
      event.preventDefault();
      action();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);
}
