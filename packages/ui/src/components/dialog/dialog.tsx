import { AlertDialog as BaseAlertDialog } from '@base-ui/react/alert-dialog';
import { Dialog as BaseDialog } from '@base-ui/react/dialog';
import { type ReactNode, type RefObject, useContext } from 'react';
import { focusRing } from '../../lib/focus';
import { interactiveTransition, pressScaleSmall } from '../../lib/motion';
import { tv, type VariantProps } from '../../lib/tv';
import { CloseIcon } from '../icon/icon-catalog';
import { DialogCloseLabel } from './dialog-store';

/**
 * The modal.
 *
 * It stops the work and does not give it back until something is decided, so
 * it only opens when there is genuinely something to decide. What resolves on
 * the screen stays on the screen; what is told and leaves goes to a corner
 * toast.
 *
 * The shape never changes -- header, body with its own scroll, footer with
 * the primary action on the right -- and the only thing that varies is the
 * width, which follows from the purpose:
 *
 *   sm  420 px   confirming something reversible;
 *   md  480 px   the irreversible, which also asks to be typed to unlock;
 *   lg  620 px   a short form, or the outcome of a long process;
 *   xl  1040 px  choosing in a table: a project, a release, a member.
 */
const dialog = tv({
  slots: {
    backdrop: [
      'fixed inset-0 z-50 bg-scrim',
      'transition-[opacity] duration-fast',
      'data-starting-style:opacity-0 data-starting-style:ease-entrance',
      'data-ending-style:opacity-0 data-ending-style:ease-exit',
    ],
    viewport: 'fixed inset-0 z-50 flex items-center justify-center p-6',
    popup: [
      'relative flex max-h-full w-full flex-col overflow-hidden',
      'rounded-xl border border-border bg-surface shadow-dialog',
      focusRing,
      /*
       * It appears growing a touch from the centre. It neither rises nor
       * drops: a modal comes from nowhere on the screen, it interrupts.
       */
      'transition-[scale,opacity] duration-fast',
      'data-starting-style:scale-97 data-starting-style:opacity-0',
      'data-starting-style:ease-entrance',
      'data-ending-style:scale-97 data-ending-style:opacity-0',
      'data-ending-style:ease-exit',
    ],
    // The air below comes from the header, not the body, so a modal without
    // a body -- a confirmation -- does not sit with its text on the footer.
    header: 'flex shrink-0 items-start gap-3.25 px-5.5 pt-5 pb-4',
    heading: 'flex min-w-0 flex-1 flex-col gap-1.25 pt-0.5',
    title: 'text-fg text-section',
    description: 'text-body text-fg-tertiary',
    close: [
      'flex size-6.5 shrink-0 items-center justify-center rounded-sm',
      'text-fg-subtle hover:bg-surface-hover hover:text-fg',
      interactiveTransition,
      pressScaleSmall,
      focusRing,
    ],
    // The body is the only part that scrolls: header and footer stay put.
    body: 'min-h-0 flex-1 overflow-y-auto px-5.5 pb-4.5',
    footer: [
      'flex shrink-0 flex-wrap items-center gap-2.25',
      'border-border-subtle border-t bg-surface-raised px-5.5 py-4',
    ],
    // On the left, whatever is not an action: the shortcut, the caveat.
    hint: 'text-fg-subtle text-meta',
    actions: 'ms-auto flex items-center gap-2.25',
  },
  variants: {
    size: {
      sm: { popup: 'max-w-dialog-sm' },
      md: { popup: 'max-w-dialog-md' },
      lg: { popup: 'max-w-dialog-lg' },
      xl: { popup: 'max-w-dialog-xl' },
    },
  },
  defaultVariants: { size: 'sm' },
});

const styles = dialog();

export type DialogSize = NonNullable<VariantProps<typeof dialog>['size']>;
/** How weighty a confirmation is: `danger` turns its action red. */
export type DialogTone = 'brand' | 'success' | 'warning' | 'danger';

export interface DialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /**
   * Called when the exit animation has finished. That is the moment to
   * unmount it: earlier and it would be seen vanishing in one frame.
   */
  onClosed?: () => void;
  size?: DialogSize;
  /**
   * Turns the modal into an alert: it does not close on an outside click and
   * has to be answered. For the destructive and the irreversible -- not for
   * a long form, where losing what was typed to a stray click is worse.
   */
  alert?: boolean;
  /** What receives focus on open. In the destructive, the way out. */
  initialFocus?: boolean | RefObject<HTMLElement | null>;
  children: ReactNode;
  className?: string;
}

/**
 * The box. Inside go `DialogHeader`, `DialogBody` and `DialogFooter`, in that
 * order; the rest -- trapped focus, locked scroll, Escape, focus returning
 * to where it was -- comes from Base UI.
 */
export function Dialog({
  open,
  onOpenChange,
  onClosed,
  size,
  alert,
  initialFocus,
  children,
  className,
}: DialogProps) {
  const Root = alert ? BaseAlertDialog.Root : BaseDialog.Root;

  return (
    <Root
      open={open}
      onOpenChange={onOpenChange}
      onOpenChangeComplete={(isOpen) => {
        if (!isOpen) {
          onClosed?.();
        }
      }}
    >
      <BaseDialog.Portal>
        <BaseDialog.Backdrop className={styles.backdrop()} />
        <BaseDialog.Viewport className={styles.viewport()}>
          <BaseDialog.Popup
            className={dialog({ size }).popup({ className })}
            initialFocus={initialFocus}
          >
            {children}
          </BaseDialog.Popup>
        </BaseDialog.Viewport>
      </BaseDialog.Portal>
    </Root>
  );
}

Dialog.displayName = 'Dialog';

export interface DialogHeaderProps {
  title: ReactNode;
  /** One line of context: where it comes from, what will happen, to what. */
  description?: ReactNode;
  /**
   * The dismiss cross. Removed when there is no neutral way out -- a
   * decision that must be taken -- so that closing is always an explicit
   * answer.
   */
  dismissible?: boolean;
  children?: ReactNode;
}

export function DialogHeader({
  title,
  description,
  dismissible = true,
  children,
}: DialogHeaderProps) {
  const closeLabel = useContext(DialogCloseLabel);
  return (
    <div className={styles.header()}>
      <div className={styles.heading()}>
        <BaseDialog.Title className={styles.title()}>{title}</BaseDialog.Title>
        {description ? (
          <BaseDialog.Description className={styles.description()}>
            {description}
          </BaseDialog.Description>
        ) : null}
        {children}
      </div>

      {dismissible ? (
        <BaseDialog.Close className={styles.close()} aria-label={closeLabel}>
          <CloseIcon size="sm" aria-hidden="true" />
        </BaseDialog.Close>
      ) : null}
    </div>
  );
}

DialogHeader.displayName = 'DialogHeader';

export interface DialogBodyProps {
  children: ReactNode;
  className?: string;
}

export function DialogBody({ children, className }: DialogBodyProps) {
  return <div className={styles.body({ className })}>{children}</div>;
}

DialogBody.displayName = 'DialogBody';

export interface DialogFooterProps {
  /** What is not an action: "Esc cancels", "4 characters left". */
  hint?: ReactNode;
  /** The actions, primary last: rightmost of all. */
  children: ReactNode;
}

export function DialogFooter({ hint, children }: DialogFooterProps) {
  return (
    <div className={styles.footer()}>
      {hint ? <span className={styles.hint()}>{hint}</span> : null}
      <div className={styles.actions()}>{children}</div>
    </div>
  );
}

DialogFooter.displayName = 'DialogFooter';

/**
 * Closes the modal from inside. Used with `render` so the way out is a
 * system button and not a different one:
 *
 * ```tsx
 * <DialogClose render={<Button variant="secondary">Cancel</Button>} />
 * ```
 */
export const DialogClose = BaseDialog.Close;
