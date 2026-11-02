import { cn } from '@rustrak/ui';
import type { ReactNode } from 'react';

/** A panel of a detail page's reading column: a heading, what it shows, its content. */
export function Panel({
  title,
  hint,
  flush,
  children,
}: {
  title: string;
  hint: string;
  /** Rows that run to the panel's edges. */
  flush?: boolean;
  children: ReactNode;
}) {
  return (
    <section className="flex min-w-0 flex-col rounded-2xl border border-border-subtle bg-surface">
      <header className="px-5 pt-4">
        <h2 className="text-card-title text-fg">{title}</h2>
        <p className="mt-0.5 text-fg-subtle text-meta">{hint}</p>
      </header>
      <div className={cn('min-w-0 pt-3 pb-4', flush ? null : 'px-5')}>
        {children}
      </div>
    </section>
  );
}

/** One block of a detail page's rail: a mono heading over its content. */
export function RailBlock({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <section className="border-border-divider border-b px-5 py-4 last:border-b-0">
      <h2 className="mb-3 text-column text-fg-meta">{title}</h2>
      {children}
    </section>
  );
}
