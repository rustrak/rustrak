import type { ComponentType, SVGProps } from 'react';
import { tv, type VariantProps } from '../../lib/tv';

/**
 * The system's icon contract. No component knows which library draws the
 * strokes: only this type and the names in `icon-catalog.ts`.
 *
 * Size and stroke are not passed to the library, they are imposed with CSS from
 * the tokens. A CSS rule beats the `width`, `height` and `stroke-width`
 * attributes any icon library paints into the SVG, so the adapter does not
 * depend on the library accepting one prop or another.
 */
export const iconVariants = tv({
  base: 'inline-block shrink-0 [stroke-width:var(--stroke-icon)]',
  variants: {
    size: {
      xs: 'size-icon-xs',
      sm: 'size-icon-sm',
      md: 'size-icon-md',
      lg: 'size-icon-lg',
      xl: 'size-icon-xl',
      '2xl': 'size-icon-2xl',
      '3xl': 'size-icon-3xl',
    },
  },
  defaultVariants: { size: 'lg' },
});

export interface IconProps
  extends Omit<SVGProps<SVGSVGElement>, 'children' | 'ref'>,
    VariantProps<typeof iconVariants> {}

export type IconComponent = ComponentType<IconProps>;

export type IconSize = NonNullable<VariantProps<typeof iconVariants>['size']>;
