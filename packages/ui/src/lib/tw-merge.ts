import { extendTailwindMerge } from 'tailwind-merge';
import type { TWMergeConfig } from 'tailwind-variants/config';
import {
  colorTokens,
  durationTokens,
  easeTokens,
  fontTokens,
  radiusTokens,
  shadowTokens,
  spacingTokens,
  textTokens,
} from './tokens';

/**
 * `styles/tokens.css` resets Tailwind's namespaces and publishes its own.
 * tailwind-merge still believes in the factory scale, so it has to be taught
 * ours: otherwise `text-control` (a size) and `text-fg-muted` (a colour) land
 * in the same class group and one swallows the other.
 */
export const twMergeConfig = {
  extend: {
    theme: {
      color: [...colorTokens],
      text: [...textTokens],
      spacing: [...spacingTokens],
      radius: [...radiusTokens],
      shadow: [...shadowTokens],
      font: [...fontTokens],
      ease: [...easeTokens],
    },
    classGroups: {
      // Duration has no theme key in tailwind-merge -- it only accepts
      // numbers. Without this, `duration-fast` would not know it overrides
      // `duration-slow`.
      duration: [{ duration: [...durationTokens] }],
    },
  },
} satisfies TWMergeConfig;

export const twMerge = extendTailwindMerge(twMergeConfig);
