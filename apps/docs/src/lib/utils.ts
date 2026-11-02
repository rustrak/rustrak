import { type ClassValue, clsx } from 'clsx';
import { extendTailwindMerge } from 'tailwind-merge';

/**
 * Plain tailwind-merge reads an unknown `text-*` as a colour, so `text-display`
 * and `text-fg` would collapse into whichever came last. The type sizes are
 * named here: the site's display scale (`app/display.css`) and the design
 * system's own (`textTokens` in `@rustrak/ui`).
 */
const twMerge = extendTailwindMerge({
  extend: {
    theme: {
      text: [
        'display',
        'headline',
        'lead',
        'eyebrow',
        'page-title',
        'title',
        'section',
        'card-title',
        'body',
        'value',
        'control',
        'label',
        'meta',
        'hint',
        'code',
        'mono',
        'mono-sm',
        'column',
        'badge',
        'tag',
        'kbd',
        'numeric',
        'numeric-lg',
      ],
    },
  },
});

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
