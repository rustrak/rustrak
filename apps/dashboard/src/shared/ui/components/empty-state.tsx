import { EmptyIcon, Text } from '@rustrak/ui';
import type { ReactNode } from 'react';

/** A page with nothing to show yet: what is missing and how to get it. */
export function EmptyState({
  title,
  description,
}: {
  title: ReactNode;
  description: ReactNode;
}) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 px-page-gutter py-16 text-center">
      <EmptyIcon size="xl" className="mb-2 text-fg-ghost" />
      <Text variant="body">{title}</Text>
      <Text variant="meta" tone="subtle" className="max-w-md">
        {description}
      </Text>
    </div>
  );
}
