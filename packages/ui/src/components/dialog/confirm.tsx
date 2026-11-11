import { type ReactNode, useState } from 'react';
import { Button } from '../button/button';
import { Field, FieldLabel } from '../field/field';
import { Input } from '../input/input';
import {
  DialogBody,
  DialogClose,
  DialogFooter,
  DialogHeader,
  type DialogTone,
} from './dialog';
import { createDialog } from './dialog-store';

/**
 * The two eternal questions, pre-assembled.
 *
 * `confirm` is an alert dialog: it does not close on an outside click, has
 * no cross, and focus lands on the way out, because the only way forward is
 * to answer. `alert` only tells something that must be read. Everything else
 * -- choosing in a table, filling a form, showing a long process's outcome
 * -- is written with `createDialog`.
 */

export interface ConfirmOptions {
  /** What will happen, subject included: "Delete the RUSTRAK-1042 issue". */
  title: string;
  /** What it takes with it and what can be recovered. This is what is read. */
  description?: ReactNode;
  /** Red only when something is destroyed: it colours the action. */
  tone?: DialogTone;
  /** The action's label, in the title's own verb: "Delete". */
  confirmLabel?: string;
  cancelLabel?: string;
  /** What must be reviewed before deciding: figures, checks. */
  details?: ReactNode;
  /**
   * A word that must be typed to unlock the action. Only for what has no way
   * back -- deleting a project, purging events --: it forces reading, which
   * is the point.
   */
  phrase?: string;
}

function ConfirmContent({
  title,
  description,
  tone = 'danger',
  confirmLabel = 'Confirm',
  cancelLabel = 'Cancel',
  details,
  phrase,
  close,
}: ConfirmOptions & { close: (result?: boolean) => void }) {
  const [typed, setTyped] = useState('');
  const locked = !!phrase && typed.trim() !== phrase;
  const remaining = phrase == null ? 0 : phrase.length - typed.trim().length;

  return (
    <>
      <DialogHeader
        title={title}
        description={description}
        dismissible={false}
      />

      {details || phrase ? (
        <DialogBody>
          {details}
          {phrase ? (
            // A `Field`, which is what ties the label to the box: written by
            // hand it would end in an `htmlFor` with no partner.
            <Field className="pt-3.5 first:pt-0">
              <FieldLabel>
                Type <b className="font-mono text-fg">{phrase}</b> to continue
              </FieldLabel>
              <Input
                value={typed}
                onChange={(event) => setTyped(event.target.value)}
                autoComplete="off"
                spellCheck={false}
                size="sm"
                className="font-mono"
              />
            </Field>
          ) : null}
        </DialogBody>
      ) : null}

      <DialogFooter
        hint={locked ? `${Math.max(remaining, 0)} characters left` : undefined}
      >
        {/* The modal's first focusable element, which is where Base UI puts
            the focus on open: in the destructive, the way out comes before
            the action. */}
        <DialogClose
          render={<Button variant="secondary" aria-label={cancelLabel} />}
        >
          {cancelLabel}
        </DialogClose>
        {/* No ⏎ hint: focus starts on the way out, so Enter is a no. */}
        <Button
          variant={tone === 'danger' ? 'danger-primary' : 'primary'}
          disabled={locked}
          onClick={() => close(true)}
        >
          {confirmLabel}
        </Button>
      </DialogFooter>
    </>
  );
}

/** 420 px for the reversible; 480 for what must be typed by hand. */
const confirmDialog = createDialog<ConfirmOptions, boolean>(ConfirmContent, {
  alert: true,
  size: 'sm',
});

const confirmPhraseDialog = createDialog<ConfirmOptions, boolean>(
  ConfirmContent,
  { alert: true, size: 'md' },
);

/**
 * Asks and waits. Returns `true` only if the action was pressed: closing any
 * other way is a no.
 *
 * ```tsx
 * if (!(await confirm({ title: 'Delete the RUSTRAK-1042 issue' }))) return;
 * ```
 */
export async function confirm(options: ConfirmOptions): Promise<boolean> {
  const dialog = options.phrase ? confirmPhraseDialog : confirmDialog;

  return (await dialog.open(options)) ?? false;
}

export interface AlertOptions {
  title: string;
  description?: ReactNode;
  /** The label of the only way out. */
  closeLabel?: string;
  details?: ReactNode;
}

const alertDialog = createDialog<AlertOptions, void>(
  function AlertContent({
    title,
    description,
    closeLabel = 'Got it',
    details,
    close,
  }) {
    return (
      <>
        <DialogHeader
          title={title}
          description={description}
          dismissible={false}
        />

        {details ? <DialogBody>{details}</DialogBody> : null}

        <DialogFooter>
          <Button variant="primary" shortcut="⏎" onClick={() => close()}>
            {closeLabel}
          </Button>
        </DialogFooter>
      </>
    );
  },
  { alert: true, size: 'sm' },
);

/**
 * Tells something that has to be read, and waits until it is dismissed. If
 * it does not have to be read, it is not a modal: it is a corner toast.
 */
export async function alert(options: AlertOptions): Promise<void> {
  await alertDialog.open(options);
}
