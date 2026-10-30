import { tv } from '../../lib/tv';

/**
 * The place of something still loading, drawn at the size it will arrive at.
 *
 * Decorative and nothing else: it is `aria-hidden`, because twenty pulsing bars
 * read out one by one say nothing. What tells a screen reader that the page is
 * loading is the region that holds them, which carries `role="status"`.
 *
 * The size is the caller's (`h-*`, `w-*`), because a skeleton is only worth
 * drawing if it matches what replaces it: a bar the height of the title it
 * stands for, so the page does not jump when the title lands.
 */
const skeleton = tv({
  // The same bar the data table and the query bar draw while they load.
  base: 'block animate-pulse rounded-xs bg-surface-chip motion-reduce:animate-none',
  variants: {
    shape: {
      block: '',
      circle: 'rounded-full',
    },
  },
  defaultVariants: { shape: 'block' },
});

export interface SkeletonProps {
  shape?: 'block' | 'circle';
  className?: string;
}

export function Skeleton({ shape, className }: SkeletonProps) {
  // A span drawn as a block: a skeleton stands in for text as often as for a
  // box, and a `div` inside a heading or a paragraph is invalid markup.
  return <span aria-hidden="true" className={skeleton({ shape, className })} />;
}

Skeleton.displayName = 'Skeleton';
