import type { ReactNode } from 'react';
import { CommandGroup } from './command';

export function Section({
  heading,
  children,
}: {
  heading: string;
  children: ReactNode;
}) {
  return <CommandGroup heading={heading}>{children}</CommandGroup>;
}
