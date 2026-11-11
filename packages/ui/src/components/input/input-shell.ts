import { focusRingWithin } from '../../lib/focus';
import { interactiveTransition } from '../../lib/motion';
import { tv } from '../../lib/tv';

/**
 * The resting frame of a field, shared by the text field and the select's
 * trigger so the two read as one family: same border, same canvas, same
 * hover.
 */
export const fieldFrame = [
  'flex w-full items-center gap-2 rounded-md border',
  'border-border-field bg-canvas hover:border-border-strong',
  interactiveTransition,
];

/**
 * The box every text control shares.
 *
 * It lives apart because more than one component needs it -- the input, the
 * textarea, whatever field comes next -- and all of them have to measure,
 * frame and react exactly the same. If each copied the classes, in three
 * months there would be several similar boxes and no equal ones.
 *
 * The box takes no state props: it reads the control it wraps. Whoever sets
 * `disabled` -- the caller, a `<fieldset>` above, Base UI's `Field.Root`
 * when validating -- the box finds out the same way, and there are never two
 * sources of truth to disagree.
 *
 * Careful with `:read-only`: in CSS it is **not** only for form controls.
 * Per the specification it matches anything that is not editable -- a
 * `<span>`, a `<button>`, an icon. A bare `has-[:read-only]` painted grey
 * any field carrying a symbol or a button inside. So the attribute is
 * checked, and only on an `input` or `textarea`.
 *
 * Rule order is priority order -- they all weigh the same in CSS and the
 * last one wins: normal, read-only, invalid, disabled.
 */
export const inputShell = tv({
  slots: {
    box: [
      fieldFrame,
      'ps-2.5 pe-1.5',

      // Read-only: the field settles into the surface. It still reads, so
      // the text stays; what goes is the promise of a caret.
      'has-[input[readonly]]:border-transparent',
      'has-[input[readonly]]:bg-surface-disabled',
      'has-[textarea[readonly]]:border-transparent',
      'has-[textarea[readonly]]:bg-surface-disabled',

      // `data-invalid` from the `invalid` prop, `aria-invalid` from a form
      // library that marks the control itself.
      'has-[[data-invalid]]:border-danger',
      'has-[[aria-invalid=true]]:border-danger',

      'has-[input:disabled]:border-border-subtle',
      'has-[input:disabled]:bg-surface-disabled',
      'has-[textarea:disabled]:border-border-subtle',
      'has-[textarea:disabled]:bg-surface-disabled',
      // The forbidden cursor covers the whole box, not only the input: the
      // gap between border and text is part of the field too, and pausing
      // there with no signal is what makes a person wonder if it is broken.
      'has-[input:disabled]:cursor-not-allowed',
      'has-[textarea:disabled]:cursor-not-allowed',

      focusRingWithin,
      // A read-only field lighting up lime on focus promises a writing it
      // will not accept.
      'has-[input[readonly]]:focus-within:border-border-subtle',
      'has-[input[readonly]]:focus-within:ring-0',
      'has-[textarea[readonly]]:focus-within:border-border-subtle',
      'has-[textarea[readonly]]:focus-within:ring-0',
    ],
    control: [
      'min-w-0 flex-1 bg-transparent text-control text-fg outline-none',
      'placeholder:text-fg-placeholder',
      'read-only:cursor-default read-only:text-fg-secondary',
      'disabled:cursor-not-allowed disabled:text-fg-disabled',
    ],
    adornment: 'shrink-0 text-fg-subtle',
  },
  variants: {
    size: {
      md: { box: 'h-control-lg' },
      /** Inside dense chrome -- a filter panel, a popover -- 32 px. */
      sm: { box: 'h-control-md' },
    },
    /**
     * Figures: end-aligned and tabular, so a column of counts squares up by
     * the digit and can be compared at a glance.
     */
    numeric: { true: { control: 'text-end font-mono tabular-nums' } },
    /** Multi-line: the box grows with the text and aligns to its top. */
    multiline: {
      true: { box: 'h-auto items-start py-2', control: 'resize-none' },
    },
  },
  defaultVariants: { size: 'md' },
});
