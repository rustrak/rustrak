'use client';

import { ArrowRightIcon, Tag, type TagTone } from '@rustrak/ui';
import {
  anchor,
  HORIZONS,
  type Horizon,
  type RoadmapItem,
} from '@/lib/roadmap';
import { cn } from '@/lib/utils';
import { Reveal, Words } from '../home/reveal';
import { Live } from './overview';

const COPY: Record<Horizon, string> = {
  Now: 'Being built right now.',
  Next: 'Planned to follow.',
  Later: 'Further out. Not decided yet.',
};

/** The project's Status options, coloured by how close to shipping they are. */
const STATUS_TONE: Record<string, TagTone> = {
  'In progress': 'brand',
  'In review': 'info',
  'In design': 'warning',
  Exploring: 'neutral',
  Shipped: 'brand',
};

/** How many shipped items the closing section lists. */
const SHIPPED = 6;

export function Stages({
  open,
  shipped,
}: {
  open: RoadmapItem[];
  shipped: RoadmapItem[];
}) {
  const recent = [...shipped]
    .sort((a, b) => (b.closedAt ?? '').localeCompare(a.closedAt ?? ''))
    .slice(0, SHIPPED);

  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col px-6">
      {HORIZONS.map((horizon, index) => (
        <Section
          key={horizon}
          id={`horizon-${horizon}`}
          eyebrow={`${String(index + 1).padStart(2, '0')} ${horizon}`}
          title={COPY[horizon]}
          live={index === 0}
          quiet={horizon === 'Later'}
          items={open.filter((item) => item.horizon === horizon)}
        />
      ))}
      {recent.length > 0 ? (
        <Section
          id="shipped"
          eyebrow="Shipped"
          title="Recently shipped."
          items={recent}
        />
      ) : null}
    </div>
  );
}

/**
 * One horizon, laid out like a landing feature: the head on the left, held
 * while its items scroll past on the right.
 */
function Section({
  id,
  eyebrow,
  title,
  items,
  live,
  quiet,
}: {
  id: string;
  eyebrow: string;
  title: string;
  items: RoadmapItem[];
  live?: boolean;
  quiet?: boolean;
}) {
  return (
    <section
      id={id}
      className="grid scroll-mt-24 gap-10 border-border-subtle border-t py-20 lg:grid-cols-[minmax(0,2fr)_minmax(0,3fr)] lg:gap-16"
    >
      <header className="flex flex-col gap-5 lg:sticky lg:top-28 lg:self-start">
        <Reveal y={12} className="flex items-center gap-2.5">
          {live ? <Live /> : null}
          <p className="font-mono text-fg-brand text-mono uppercase tracking-wider">
            {eyebrow}
          </p>
        </Reveal>
        <Words
          text={title}
          delay={0.1}
          className={cn(
            'text-balance text-headline',
            quiet ? 'text-fg-subtle' : 'text-fg',
          )}
        />
        <Reveal delay={0.3}>
          <p className="text-fg-meta text-meta">
            {items.length} {items.length === 1 ? 'item' : 'items'}
          </p>
        </Reveal>
      </header>

      {items.length > 0 ? (
        <ol className="flex flex-col gap-3">
          {items.map((item, i) => (
            <Row key={item.id} item={item} index={i} quiet={quiet} />
          ))}
        </ol>
      ) : (
        <p className="text-fg-subtle text-value">Nothing here yet.</p>
      )}
    </section>
  );
}

/** One item as a card, opening the issue or pull request on GitHub. */
function Row({
  item,
  index,
  quiet,
}: {
  item: RoadmapItem;
  index: number;
  quiet?: boolean;
}) {
  return (
    <Reveal
      as="li"
      delay={0.15 + Math.min(index, 5) * 0.06}
      className="rounded-2xl border border-border-subtle bg-panel transition-colors hover:border-border"
    >
      <a
        id={anchor(item)}
        href={item.url}
        target="_blank"
        rel="noreferrer"
        className="group flex scroll-mt-32 items-start justify-between gap-6 p-6"
      >
        <span className="flex min-w-0 flex-col gap-3">
          <span
            className={cn(
              'text-balance text-section transition-colors group-hover:text-fg-brand',
              quiet ? 'text-fg-subtle' : 'text-fg',
            )}
          >
            {item.title}
          </span>
          <span className="flex flex-wrap items-center gap-3">
            {item.status ? (
              <Tag tone={STATUS_TONE[item.status] ?? 'neutral'}>
                {item.status}
              </Tag>
            ) : null}
            {item.area ? <Tag variant="soft">{item.area}</Tag> : null}
            {item.number ? (
              <span className="font-mono text-fg-meta text-mono-sm">
                #{item.number}
              </span>
            ) : null}
          </span>
        </span>
        <ArrowRightIcon
          aria-hidden
          className="mt-1 size-4 shrink-0 -rotate-45 text-fg-subtle transition-[color,translate] group-hover:translate-x-0.5 group-hover:text-fg-brand"
        />
      </a>
    </Reveal>
  );
}
