import { Command as CommandPrimitive } from 'cmdk';
import type { ComponentProps } from 'react';
import { cn } from '@/shared/lib/utils';

/**
 * The cmdk parts the palette uses, drawn as @rustrak/ui's menu panel draws its
 * rows: the same item height, ink and highlight.
 */

export function Command({
  className,
  ...props
}: ComponentProps<typeof CommandPrimitive>) {
  return (
    <CommandPrimitive
      className={cn('flex min-h-0 flex-col text-fg', className)}
      {...props}
    />
  );
}

export function CommandList({
  className,
  ...props
}: ComponentProps<typeof CommandPrimitive.List>) {
  return (
    <CommandPrimitive.List
      className={cn(
        'scroll-py-1.5 overflow-x-hidden overflow-y-auto outline-none',
        className,
      )}
      {...props}
    />
  );
}

export function CommandEmpty({
  className,
  ...props
}: ComponentProps<typeof CommandPrimitive.Empty>) {
  return (
    <CommandPrimitive.Empty
      className={cn('text-center text-body text-fg-subtle', className)}
      {...props}
    />
  );
}

/**
 * Headings read like a menu's group label: the column face, in sentence case.
 * `**:` reaches the `[cmdk-group-heading]` element cmdk renders for us, which
 * is the only handle we get on it.
 */
export function CommandGroup({
  className,
  ...props
}: ComponentProps<typeof CommandPrimitive.Group>) {
  return (
    <CommandPrimitive.Group
      className={cn(
        'overflow-hidden',
        '**:[[cmdk-group-heading]]:px-3 **:[[cmdk-group-heading]]:pt-3',
        '**:[[cmdk-group-heading]]:pb-1.5 **:[[cmdk-group-heading]]:font-mono',
        '**:[[cmdk-group-heading]]:text-column',
        '**:[[cmdk-group-heading]]:text-fg-meta',
        className,
      )}
      {...props}
    />
  );
}

/**
 * cmdk writes `data-selected="false"` on every row, and Tailwind's bare
 * `data-selected:` variant matches the attribute's *presence*, so selection is
 * tested against `=true` explicitly.
 */
export function CommandItem({
  className,
  ...props
}: ComponentProps<typeof CommandPrimitive.Item>) {
  return (
    <CommandPrimitive.Item
      className={cn(
        'group/command-item relative flex cursor-default items-center gap-2.5 rounded-md px-2.5',
        'text-control text-fg-muted outline-none select-none transition-none',
        'data-[selected=true]:bg-surface-selected data-[selected=true]:text-fg',
        'data-[disabled=true]:pointer-events-none data-[disabled=true]:text-fg-disabled',
        className,
      )}
      {...props}
    />
  );
}
