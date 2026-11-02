import { createTV } from 'tailwind-variants/config';
import { twMergeConfig } from './tw-merge';

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
export const tv = createTV({ twMergeConfig });

export type { VariantProps } from 'tailwind-variants';
