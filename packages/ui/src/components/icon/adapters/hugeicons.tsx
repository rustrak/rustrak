import { HugeiconsIcon, type IconSvgElement } from '@hugeicons/react';
import { type IconComponent, type IconProps, iconVariants } from '../icon';

/**
 * Adapter from Hugeicons (stroke-rounded, free set) to the `IconComponent`
 * contract.
 *
 * With `hugeicons-symbols.ts` beside it, this directory is the only place in
 * the package that touches `@hugeicons/*`. Switching icon library means writing
 * another adapter with this signature and another symbol list, and pointing
 * `icon-catalog.ts` at them; not one component finds out.
 *
 * What it normalises:
 *   - size and stroke become tokens rather than library props (the CSS in
 *     `iconVariants` beats the attributes Hugeicons paints);
 *   - `currentColor`, so an icon takes the colour of the text it sits in;
 *   - the icon is decorative by default (`aria-hidden`). Give it an
 *     `aria-label` and it becomes `role="img"` and is announced;
 *   - it is never reachable by tab.
 */
export function fromHugeicons(
  displayName: string,
  source: IconSvgElement,
): IconComponent {
  // `strokeWidth` is dropped, not forwarded: the stroke is a token, imposed
  // by `iconVariants`, and Hugeicons only takes it as a number anyway.
  function Icon({
    size,
    className,
    strokeWidth: _strokeWidth,
    ...props
  }: IconProps) {
    const labelled = props['aria-label'] != null;

    return (
      <HugeiconsIcon
        icon={source}
        color="currentColor"
        aria-hidden={labelled ? undefined : true}
        role={labelled ? 'img' : undefined}
        focusable="false"
        {...props}
        className={iconVariants({ size, className })}
      />
    );
  }

  Icon.displayName = displayName;

  return Icon;
}
