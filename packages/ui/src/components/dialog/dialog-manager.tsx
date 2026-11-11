import {
  type ReactNode,
  useCallback,
  useEffect,
  useSyncExternalStore,
} from 'react';
import { Dialog } from './dialog';
import {
  close,
  DialogCloseLabel,
  type Entry,
  getServerSnapshot,
  getSnapshot,
  registerHost,
  remove,
  subscribe,
} from './dialog-store';

function DialogEntry({ entry }: { entry: Entry }) {
  const { Component, props, options } = entry;

  const handleClose = useCallback(
    (result?: unknown) => close(entry.id, result),
    [entry.id],
  );

  return (
    <Dialog
      open={entry.open}
      onOpenChange={(next) => {
        if (!next) {
          // Escape, the cross or a click outside: closed with no answer.
          close(entry.id, undefined);
        }
      }}
      onClosed={() => remove(entry.id)}
      size={options.size}
      alert={options.alert}
    >
      <Component {...props} close={handleClose} />
    </Dialog>
  );
}

export interface DialogProviderProps {
  /** The dismiss cross's accessible name, in the reader's language. */
  closeLabel: string;
  children?: ReactNode;
}

/**
 * Goes once, at the application's root. It is what mounts the modals opened
 * with `open()`, inside the tree and below every provider.
 */
export function DialogProvider({ closeLabel, children }: DialogProviderProps) {
  const open = useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);

  return (
    <DialogCloseLabel.Provider value={closeLabel}>
      {children}
      <DialogOutlet entries={open} />
    </DialogCloseLabel.Provider>
  );
}

DialogProvider.displayName = 'DialogProvider';

function DialogOutlet({ entries: open }: { entries: Entry[] }) {
  // Only so development can warn that the provider is missing.
  useEffect(() => registerHost(), []);

  return (
    <>
      {open.map((entry) => (
        <DialogEntry key={entry.id} entry={entry} />
      ))}
    </>
  );
}
