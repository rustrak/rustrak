import { createTV } from 'tailwind-variants/lite';
import { twMerge } from './tw-merge';

/**
 * `tv` is the only way to describe what a component looks like. Each recipe
 * declares its variants and, when the component has several parts, its slots.
 *
 * House rules for writing one:
 *
 *   - token-backed utilities only (`bg-surface`, `h-control-md`,
 *     `text-control`). Never `bg-[#1a1a1a]`, `h-[32px]` or `text-[12.5px]`;
 *   - state is read from the attributes Base UI exposes (`data-selected:`,
 *     `data-disabled:`, `data-highlighted:`), never from props threaded down
 *     by hand;
 *   - focus always goes through the system's `focusRing`.
 */
// The lite entry with `cn`'s own merger. The default entry ships a second,
// bundled copy of tailwind-merge (~77 KB): the same engine, loaded twice.
export const tv = createTV({ twMerge });

export type { VariantProps } from 'tailwind-variants';
