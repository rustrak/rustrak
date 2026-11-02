import type { ReactNode } from 'react';
import { tv, type VariantProps } from '../../lib/tv';

/**
 * A status label: ERROR, FATAL, REGRESSION, STABLE, Owner.
 *
 * It defaults to a dot and a word with no box, and that is a settled
 * decision: an issue list is already a grid of rows and rules, and
 * putting a filled pill on every row turned severity into confetti. The dot
 * is what the eye scans down a column; the word, in sentence case whatever
 * case it was handed in, is what it reads. Tracked mono capitals made every
 * row shout.
 *
 * `soft` brings the tinted background back for the one case that needs it: a
 * label sitting on its own, away from a column of its peers -- a role beside a
 * member's name, a state on a release card -- where there is nothing nearby to
 * read it against.
 *
 * The tone never carries the meaning alone. Every tag is a word, and the word
 * survives the colour being unavailable.
 */
const tag = tv({
  base: 'inline-flex shrink-0 items-center gap-1.5 text-tag',
  variants: {
    tone: {
      error: 'text-sev-error-fg',
      warning: 'text-sev-warning-fg',
      info: 'text-sev-info-fg',
      brand: 'text-fg-brand',
      neutral: 'text-fg-tertiary',
    },
    variant: {
      text: "before:size-1.5 before:shrink-0 before:rounded-pill before:bg-current before:content-['']",
      soft: 'h-5 rounded-sm px-1.5',
    },
  },
  compoundVariants: [
    { variant: 'soft', tone: 'error', class: 'bg-sev-error-surface' },
    { variant: 'soft', tone: 'warning', class: 'bg-sev-warning-surface' },
    { variant: 'soft', tone: 'info', class: 'bg-sev-info-surface' },
    { variant: 'soft', tone: 'brand', class: 'bg-success-surface' },
    { variant: 'soft', tone: 'neutral', class: 'bg-surface-chip' },
  ],
  defaultVariants: { tone: 'neutral', variant: 'text' },
});

export type TagTone = NonNullable<VariantProps<typeof tag>['tone']>;

export interface TagProps extends VariantProps<typeof tag> {
  children: ReactNode;
  className?: string;
}

export function Tag({ tone, variant, className, children }: TagProps) {
  return (
    <span className={tag({ tone, variant, className })}>
      {/* `inline-block`, because `::first-letter` only reaches a block. */}
      <span className="inline-block lowercase first-letter:uppercase">
        {children}
      </span>
    </span>
  );
}

Tag.displayName = 'Tag';
