'use client';

import { Tag } from '@rustrak/ui';
import { useLenis } from 'lenis/react';
import * as m from 'motion/react-m';
import { anchor, HORIZONS, type RoadmapItem } from '@/lib/roadmap';
import { Backdrop } from '../editorial/intro';
import { EASE, VIEW } from '../landing/motion';
import { useSmoothScrolling } from '../landing/smooth-scroll';

/** Cards shown per column before the rest folds into a count. */
const PER_COLUMN = 4;

/**
 * The whole roadmap as a board on the landing's lime backdrop: a column per
 * horizon, a card per item, read left to right from in flight to furthest
 * out. A card scrolls to its full entry below.
 */
export function Overview({ items }: { items: RoadmapItem[] }) {
  const lenis = useLenis();
  const smooth = useSmoothScrolling();

  const go = (target: string) => {
    if (lenis && smooth)
      lenis.scrollTo(target, { offset: -120, duration: 1.6 });
    else document.querySelector(target)?.scrollIntoView({ behavior: 'smooth' });
  };

  let n = 0;

  return (
    <m.div
      initial={{ opacity: 0, y: 48, scale: 0.96 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      transition={{ duration: 1.4, ease: EASE, delay: 0.6 }}
    >
      <Backdrop>
        <m.div
          initial={{ opacity: 0, y: 40 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 1.5, ease: EASE, delay: 0.85 }}
          className="m-3 grid gap-3 rounded-2xl border border-white/15 bg-panel p-3 shadow-dialog sm:m-8 md:grid-cols-3 lg:m-12"
        >
          {HORIZONS.map((horizon, h) => {
            const column = items.filter((item) => item.horizon === horizon);
            const rest = column.length - PER_COLUMN;
            return (
              <div
                key={horizon}
                className="flex flex-col gap-2 rounded-xl bg-aside p-3"
              >
                <div className="flex items-center gap-2 px-1 pb-1">
                  {h === 0 ? <Live /> : null}
                  <span className="text-fg text-label">{horizon}</span>
                  <span className="text-fg-meta text-meta tabular-nums">
                    {column.length}
                  </span>
                </div>
                {column.slice(0, PER_COLUMN).map((item) => {
                  const delay = 1.1 + n++ * 0.06;
                  return (
                    <m.button
                      key={item.id}
                      type="button"
                      onClick={() => go(`#${anchor(item)}`)}
                      initial={{ opacity: 0, y: 16 }}
                      whileInView={{ opacity: 1, y: 0 }}
                      viewport={VIEW}
                      transition={{ duration: 0.9, ease: EASE, delay }}
                      className="flex flex-col items-start gap-2 rounded-lg border border-border-subtle bg-surface p-3 text-start transition-colors hover:border-border-strong hover:bg-surface-hover"
                    >
                      <span className="line-clamp-2 text-fg text-control">
                        {item.title}
                      </span>
                      {item.area ? <Tag variant="soft">{item.area}</Tag> : null}
                    </m.button>
                  );
                })}
                {rest > 0 ? (
                  <button
                    type="button"
                    onClick={() => go(`#horizon-${horizon}`)}
                    className="rounded-lg px-1 py-1.5 text-start text-fg-meta text-meta transition-colors hover:text-fg"
                  >
                    +{rest} more
                  </button>
                ) : null}
                {column.length === 0 ? (
                  <p className="px-1 py-2 text-fg-meta text-meta">
                    Nothing here yet.
                  </p>
                ) : null}
              </div>
            );
          })}
        </m.div>
      </Backdrop>
    </m.div>
  );
}

/** A small lime dot that breathes: the one horizon that is moving. */
export function Live() {
  return (
    <span className="relative flex size-1.5">
      <span className="absolute -inset-1 animate-pulse rounded-full bg-surface-brand/25 motion-reduce:animate-none" />
      <span className="relative size-1.5 rounded-full bg-surface-brand" />
    </span>
  );
}
