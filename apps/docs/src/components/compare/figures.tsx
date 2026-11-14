import type { ReactNode } from 'react';
import { mb, num, type Run } from '@/lib/compare';
import { cn } from '@/lib/utils';
import { Reveal } from '../home/reveal';

type Row = { label: string; hint?: string; cell: (r: Run) => ReactNode };

const ROWS: Row[] = [
  { label: 'Version', cell: (r) => r.version },
  { label: 'Containers', cell: (r) => r.containers },
  {
    label: 'Memory at rest',
    hint: 'Median of the 15 s before the burst',
    cell: (r) => mb(r.ram.idle),
  },
  {
    label: 'Memory at peak',
    hint: 'Highest second, burst to end',
    cell: (r) => mb(r.ram.peak),
  },
  {
    label: 'Accepted per second',
    hint: '2xx answers over the burst',
    cell: (r) => num(r.acceptPerS),
  },
  { label: 'p99 latency', cell: (r) => `${r.p99Ms} ms` },
  {
    label: 'Stored per second',
    hint: 'Queryable in the database',
    cell: (r) => num(r.storedPerS),
  },
  {
    label: 'All 20,000 stored after',
    cell: (r) => `${r.storedSecs} s`,
  },
  { label: 'CPU per event', cell: (r) => `${r.cpuMsPerEvent} ms` },
  { label: 'Disk after the run', cell: (r) => mb(r.diskMb) },
  { label: 'Images to pull', cell: (r) => mb(r.imageMb) },
  {
    label: 'Ready after start',
    hint: 'Install and image pulls excluded',
    cell: (r) => (r.readySecs < 1 ? '< 1 s' : `${r.readySecs} s`),
  },
];

/** Metrics down the side, one column per run; Rustrak's columns lit. */
export function ResultsTable({ runs }: { runs: Run[] }) {
  return (
    <Reveal
      y={40}
      className="overflow-x-auto rounded-3xl border border-border-subtle"
    >
      <table
        className="w-full border-collapse text-start"
        style={{ minWidth: `${14 + runs.length * 9}rem` }}
      >
        <thead>
          <tr className="border-border-subtle border-b">
            <th className="p-4 sm:p-5" />
            {runs.map((r) => (
              <th
                key={r.id}
                scope="col"
                className={cn(
                  'p-4 text-start align-bottom sm:p-5',
                  r.rustrak && 'bg-surface',
                )}
              >
                <span
                  className={cn(
                    'block text-section',
                    r.rustrak ? 'text-fg-brand' : 'text-fg',
                  )}
                >
                  {r.name}
                </span>
                <span className="block font-normal text-fg-meta text-meta">
                  {r.variant}
                </span>
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {ROWS.map((row) => (
            <tr
              key={row.label}
              className="border-border-divider border-b last:border-b-0"
            >
              <th scope="row" className="p-4 text-start font-normal sm:p-5">
                <span className="block text-fg-secondary text-value">
                  {row.label}
                </span>
                {row.hint ? (
                  <span className="block text-fg-meta text-meta">
                    {row.hint}
                  </span>
                ) : null}
              </th>
              {runs.map((r) => (
                <td
                  key={r.id}
                  className={cn(
                    'p-4 font-mono text-mono tabular-nums sm:p-5',
                    r.rustrak ? 'bg-surface text-fg' : 'text-fg-subtle',
                  )}
                >
                  {row.cell(r)}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </Reveal>
  );
}

/** A large figure with what it means under it. */
export function Stat({
  value,
  label: text,
  delay,
}: {
  value: string;
  label: string;
  delay?: number;
}) {
  return (
    <Reveal
      delay={delay}
      className="flex flex-col gap-2 rounded-3xl border border-border-subtle bg-panel p-6 sm:p-8"
    >
      <span className="whitespace-nowrap text-fg-brand text-headline">
        {value}
      </span>
      <span className="text-pretty text-fg-subtle text-value">{text}</span>
    </Reveal>
  );
}

/** A section heading inside a comparison page: an `h2` and an optional lead. */
export function Heading({
  id,
  title,
  lead,
}: {
  id?: string;
  title: string;
  lead?: ReactNode;
}) {
  return (
    <Reveal y={16} className="flex max-w-2xl flex-col gap-4">
      <h2 id={id} className="text-balance text-fg text-headline">
        {title}
      </h2>
      {lead ? (
        <p className="text-pretty text-fg-subtle text-lead">{lead}</p>
      ) : null}
    </Reveal>
  );
}

/** Questions as a definition list; the page puts the same pairs in JSON-LD. */
export function Faq({ items }: { items: { q: string; a: string }[] }) {
  return (
    <dl className="mt-10 grid gap-4 md:grid-cols-2">
      {items.map((item, i) => (
        <Reveal
          key={item.q}
          delay={i * 0.05}
          className="rounded-3xl border border-border-subtle p-6 sm:p-8"
        >
          <dt className="text-fg text-section">{item.q}</dt>
          <dd className="mt-3 text-pretty text-fg-subtle text-value">
            {item.a}
          </dd>
        </Reveal>
      ))}
    </dl>
  );
}

/** A chart or table in a panel, with an optional caption under it. */
export function Figure({
  children,
  caption,
  className,
}: {
  children: ReactNode;
  caption?: ReactNode;
  className?: string;
}) {
  return (
    <Reveal
      y={40}
      className={`rounded-3xl border border-border-subtle bg-panel p-5 sm:p-8 ${className ?? ''}`}
    >
      <figure className="flex flex-col gap-6">
        {children}
        {caption ? (
          <figcaption className="max-w-2xl text-pretty text-fg-meta text-meta">
            {caption}
          </figcaption>
        ) : null}
      </figure>
    </Reveal>
  );
}
