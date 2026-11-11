import { cn } from '@rustrak/ui';
import type { ReactNode } from 'react';

/**
 * A page's headline figures in one strip, the cells split by hairlines: cards
 * at two heights would leave a ragged edge. `className` sets the columns.
 */
export function FigureStrip({
  className,
  children,
}: {
  className: string;
  children: ReactNode;
}) {
  return (
    <dl
      className={cn(
        'grid gap-px overflow-hidden rounded-2xl border border-border-subtle bg-border-subtle',
        className,
      )}
    >
      {children}
    </dl>
  );
}

/**
 * A rail's label/value pairs, the labels in one column. `className` sets the
 * row gap, `valueClassName` how the values read (right-aligned, mono...).
 */
export function FactList({
  facts,
  className,
  valueClassName,
}: {
  facts: readonly (readonly [string, ReactNode])[];
  className: string;
  valueClassName: string;
}) {
  return (
    <dl
      className={cn(
        'grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 text-meta',
        className,
      )}
    >
      {facts.map(([label, value]) => (
        <div key={label} className="contents">
          <dt className="text-fg-subtle">{label}</dt>
          <dd className={cn('min-w-0', valueClassName)}>{value}</dd>
        </div>
      ))}
    </dl>
  );
}

/**
 * One cell of a `FigureStrip`. A `footnote` of `null` still takes its line,
 * so a strip where some cells have one stays level; leave it out for none.
 */
export function Figure({
  label,
  value,
  footnote,
  valueClassName = 'text-fg',
}: {
  label: ReactNode;
  value: ReactNode;
  footnote?: ReactNode;
  valueClassName?: string;
}) {
  return (
    <div className="flex min-w-0 flex-col gap-1.5 bg-surface px-4 py-3.5">
      <dt className="truncate text-fg-meta text-meta">{label}</dt>
      <dd className={cn('text-title tabular-nums', valueClassName)}>{value}</dd>
      {footnote !== undefined ? (
        <dd className="truncate text-fg-subtle text-hint">{footnote ?? ' '}</dd>
      ) : null}
    </div>
  );
}
