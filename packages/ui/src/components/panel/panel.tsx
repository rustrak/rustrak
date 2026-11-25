import type { ReactNode } from 'react';
import { tv } from '../../lib/tv';
import { Skeleton } from '../skeleton/skeleton';

const panel = tv({
  slots: {
    root: 'flex h-full min-w-0 flex-col rounded-2xl border border-border-subtle bg-surface',
    header: 'flex flex-wrap items-start justify-between gap-3 px-5 pt-4',
    title: 'text-card-title text-fg',
    hint: 'mt-0.5 text-meta text-fg-subtle',
    body: 'min-w-0 flex-1 px-5 pt-3 pb-4',
    footer: 'border-t border-border-divider px-5 py-3',
  },
  variants: {
    flush: { true: { body: 'px-0' } },
  },
});

export interface PanelProps {
  title: ReactNode;
  /** What it shows, under the title. */
  hint?: ReactNode;
  /** Controls beside the heading. */
  actions?: ReactNode;
  footer?: ReactNode;
  /** Rows that run to the panel's edges. */
  flush?: boolean;
  children: ReactNode;
}

/** A panel of a page: a heading, what it shows, its content, maybe a footer. */
export function Panel({
  title,
  hint,
  actions,
  footer,
  flush,
  children,
}: PanelProps) {
  const s = panel({ flush });
  return (
    <section className={s.root()}>
      <header className={s.header()}>
        <div className="min-w-0">
          <h2 className={s.title()}>{title}</h2>
          {hint ? <p className={s.hint()}>{hint}</p> : null}
        </div>
        {actions}
      </header>
      <div className={s.body()}>{children}</div>
      {footer ? <footer className={s.footer()}>{footer}</footer> : null}
    </section>
  );
}

const rail = tv({
  slots: {
    root: 'border-b border-border-divider px-5 py-4 last:border-b-0',
    title: 'mb-3 text-column text-fg-meta',
  },
});

/** One block of a detail page's rail: a mono heading over its content. */
export function RailBlock({
  title,
  children,
}: {
  title: ReactNode;
  children: ReactNode;
}) {
  const s = rail();
  return (
    <section className={s.root()}>
      <h2 className={s.title()}>{title}</h2>
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
