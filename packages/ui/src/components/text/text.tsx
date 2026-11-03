import { mergeProps } from '@base-ui/react/merge-props';
import { useRender } from '@base-ui/react/use-render';
import { tv, type VariantProps } from '../../lib/tv';

/**
 * Every piece of text in the system.
 *
 * `variant` chooses the role, not the size: `page-title`, `control`, `mono`.
 * That is the whole point of having it -- a component asks for the rank the
 * text has in the page and the system decides what that costs in pixels, so
 * retuning the scale is one file and not two hundred call sites.
 *
 * The element is decided separately, with `render`, because rank and tag are
 * different questions: a card title is a `<h3>` on one screen and a `<span>`
 * inside a row on another, and it should look the same in both.
 *
 * The machine roles -- `code`, `mono`, `mono-sm`, `column`, `badge`, `tag`,
 * `kbd`, and the two figures -- carry `font-mono` and `tabular-nums` on their
 * own. Anything the system produced and somebody might copy out of the page is
 * one of these.
 */
const text = tv({
  base: 'font-sans',
  variants: {
    variant: {
      'page-title': 'text-page-title',
      title: 'text-title',
      section: 'text-section',
      'card-title': 'text-card-title',
      body: 'text-body',
      value: 'text-value',
      control: 'text-control',
      label: 'text-label',
      meta: 'text-meta',
      hint: 'text-hint',
      code: 'font-mono text-code tabular-nums',
      mono: 'font-mono text-mono tabular-nums',
      'mono-sm': 'font-mono text-mono-sm tabular-nums',
      column: 'text-column tabular-nums',
      badge: 'font-mono text-badge tabular-nums',
      tag: 'text-tag',
      kbd: 'font-mono text-kbd',
      /*
       * The two figures are the exception to the mono rule, and the design
       * is explicit about it: a KPI is set in Inter at 24/600 with tabular
       * figures. At that size mono stops reading as precision and starts
       * reading as a terminal, and the thousands separator opens a hole in
       * the middle of the number. Tabular figures already give the alignment,
       * which was the only reason mono was ever wanted here.
       */
      numeric: 'text-numeric tabular-nums',
      'numeric-lg': 'text-numeric-lg tabular-nums',
    },
    tone: {
      default: 'text-fg',
      secondary: 'text-fg-secondary',
      tertiary: 'text-fg-tertiary',
      muted: 'text-fg-muted',
      subtle: 'text-fg-subtle',
      meta: 'text-fg-meta',
      ghost: 'text-fg-ghost',
      placeholder: 'text-fg-placeholder',
      disabled: 'text-fg-disabled',
      brand: 'text-fg-brand',
      'on-brand': 'text-fg-on-brand',
      error: 'text-sev-error-fg',
      warning: 'text-sev-warning-fg',
      inherit: 'text-inherit',
    },
    /** One line, clipped with an ellipsis. Needs a parent with `min-w-0`. */
    truncate: { true: 'min-w-0 truncate' },
  },
  defaultVariants: { variant: 'body', tone: 'default' },
});

export type TextVariant = NonNullable<VariantProps<typeof text>['variant']>;
export type TextTone = NonNullable<VariantProps<typeof text>['tone']>;

export interface TextProps
  extends useRender.ComponentProps<'span'>,
    VariantProps<typeof text> {}

export function Text({
  variant,
  tone,
  truncate,
  className,
  render,
  ...props
}: TextProps) {
  return useRender({
    defaultTagName: 'span',
    render,
    props: mergeProps<'span'>(
      { className: text({ variant, tone, truncate, className }) },
      props,
    ),
  });
}

Text.displayName = 'Text';
