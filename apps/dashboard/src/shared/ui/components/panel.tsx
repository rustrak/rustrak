import { cn, Skeleton } from '@rustrak/ui';
import type { ReactNode } from 'react';

/** A panel of a page: a heading, what it shows, its content, maybe a footer. */
export function Panel({
  title,
  hint,
  actions,
  footer,
  flush,
  children,
}: {
  title: string;
  hint?: ReactNode;
  /** Controls beside the heading. */
  actions?: ReactNode;
  footer?: ReactNode;
  /** Rows that run to the panel's edges. */
  flush?: boolean;
  children: ReactNode;
}) {
  return (
    <section className="flex h-full min-w-0 flex-col rounded-2xl border border-border-subtle bg-surface">
      <header className="flex flex-wrap items-start justify-between gap-3 px-5 pt-4">
        <div className="min-w-0">
          <h2 className="text-card-title text-fg">{title}</h2>
          {hint ? (
            <p className="mt-0.5 text-fg-subtle text-meta">{hint}</p>
          ) : null}
        </div>
        {actions}
      </header>
      <div className={cn('min-w-0 flex-1 pt-3 pb-4', flush ? null : 'px-5')}>
        {children}
      </div>
      {footer ? (
        <footer className="border-border-divider border-t px-5 py-3">
          {footer}
        </footer>
      ) : null}
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

/** Where a chart or a list will be, at the height it will take. */
export function PanelPlaceholder({ height }: { height: number }) {
  return (
    <div style={{ height }}>
      <Skeleton className="size-full rounded-lg" />
    </div>
  );
}
