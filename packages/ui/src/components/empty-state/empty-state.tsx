import type { ReactNode } from 'react';
import { tv } from '../../lib/tv';
import { EmptyIcon } from '../icon/icon-catalog';
import { Text } from '../text/text';

/** A page with nothing to show yet: what is missing and how to get it. */
const emptyState = tv({
  slots: {
    root: 'flex flex-1 flex-col items-center justify-center gap-2 px-page-gutter py-16 text-center',
    icon: 'mb-2 text-fg-ghost',
    description: 'max-w-md',
  },
});

export interface EmptyStateProps {
  title: ReactNode;
  description: ReactNode;
}

export function EmptyState({ title, description }: EmptyStateProps) {
  const s = emptyState();
  return (
    <div className={s.root()}>
      <EmptyIcon size="xl" className={s.icon()} />
      <Text variant="body">{title}</Text>
      <Text variant="meta" tone="subtle" className={s.description()}>
        {description}
      </Text>
    </div>
  );
}

EmptyState.displayName = 'EmptyState';
