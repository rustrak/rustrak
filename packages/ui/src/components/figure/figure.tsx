import type { ReactNode } from 'react';
import { cn } from '../../lib/cn';
import { tv } from '../../lib/tv';

const figure = tv({
  slots: {
    strip:
      'grid gap-px overflow-hidden rounded-2xl border border-border-subtle bg-border-subtle',
    cell: 'flex min-w-0 flex-col gap-1.5 bg-surface px-4 py-3.5',
    label: 'truncate text-meta text-fg-meta',
    value: 'text-title tabular-nums',
    footnote: 'truncate text-hint text-fg-subtle',
    facts: 'grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 text-meta',
    term: 'text-fg-subtle',
    fact: 'min-w-0',
  },
});

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
  return <dl className={figure().strip({ className })}>{children}</dl>;
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
  const s = figure();
  return (
    <div className={s.cell()}>
      <dt className={s.label()}>{label}</dt>
      <dd className={s.value({ className: valueClassName })}>{value}</dd>
      {footnote !== undefined ? (
        <dd className={s.footnote()}>{footnote ?? ' '}</dd>
      ) : null}
    </div>
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
  const s = figure();
  return (
    <dl className={s.facts({ className })}>
      {facts.map(([label, value]) => (
        <div key={label} className="contents">
          <dt className={s.term()}>{label}</dt>
          <dd className={cn(s.fact(), valueClassName)}>{value}</dd>
        </div>
      ))}
    </dl>
  );
}
