'use client';

import type { ReactNode } from 'react';
import { useEffect, useState } from 'react';
import { cn } from '@/lib/utils';

/**
 * Where you are in the post: a hairline of progress under the navbar, and the
 * post's own headings pinned beside the text.
 *
 * Both answer the same question — "how much of this is left, and what is
 * coming" — which is the one a reader asks four screens into an eight-minute
 * piece. Nextra's own table of contents is switched off for the blog (see
 * `content/_meta.js`), so this is not a second one competing with it; it is
 * the only one, drawn in this section's language rather than the docs theme's.
 */

export type TocItem = {
  depth: number;
  id: string;
  value: ReactNode;
};

/** Where the fixed chrome ends and the page begins. */
const NAV_OFFSET = 'var(--nextra-navbar-height, 4rem)';

/** How far below the navbar a heading counts as "the one being read". */
const ACTIVE_MARGIN = 96;

function clamp(value: number): number {
  return Math.min(1, Math.max(0, value));
}

export function ReadingRail({
  items,
  bodyId,
}: {
  items: TocItem[];
  /** The element the progress is measured against. */
  bodyId: string;
}) {
  const [progress, setProgress] = useState(0);
  const [activeId, setActiveId] = useState<string>('');

  /**
   * One listener for both readings. They are measured from the same scroll
   * position and neither is worth a second pass over the document — and a
   * single rAF-throttled handler is cheaper than the IntersectionObserver this
   * started as, which needed its own root margin tuned to the sticky navbar to
   * agree with the progress bar about where the top of the page is.
   */
  useEffect(() => {
    let frame = 0;

    const measure = () => {
      frame = 0;
      const body = document.getElementById(bodyId);
      if (!body) return;

      const rect = body.getBoundingClientRect();
      // Zero when the top of the post reaches the top of the viewport, one
      // when its last line does. `max(1, …)` keeps a post shorter than the
      // window from dividing by zero and pinning the bar at 100%.
      const scrollable = Math.max(1, rect.height - window.innerHeight);
      setProgress(clamp(-rect.top / scrollable));

      let current = '';
      for (const item of items) {
        const heading = document.getElementById(item.id);
        if (heading && heading.getBoundingClientRect().top <= ACTIVE_MARGIN) {
          current = item.id;
        }
      }
      setActiveId(current);
    };

    const onScroll = () => {
      if (!frame) frame = requestAnimationFrame(measure);
    };

    measure();
    window.addEventListener('scroll', onScroll, { passive: true });
    window.addEventListener('resize', onScroll);
    return () => {
      if (frame) cancelAnimationFrame(frame);
      window.removeEventListener('scroll', onScroll);
      window.removeEventListener('resize', onScroll);
    };
  }, [items, bodyId]);

  return (
    <>
      {/*
        Decorative: the same information is in the scrollbar, and a screen
        reader announcing a percentage on every scroll tick would be noise.
        Transformed rather than resized so it stays off the main thread.
      */}
      <div
        aria-hidden
        className="pointer-events-none fixed inset-x-0 z-30 h-px bg-transparent"
        style={{ top: NAV_OFFSET }}
      >
        <div
          className="h-full origin-left bg-surface-brand"
          style={{ transform: `scaleX(${progress})` }}
        />
      </div>

      {items.length > 0 && (
        <nav
          aria-label="On this page"
          className="sticky hidden max-h-[calc(100vh-9rem)] overflow-y-auto xl:block"
          style={{ top: `calc(${NAV_OFFSET} + 3.5rem)` }}
        >
          <p className="font-mono text-mono text-fg-meta uppercase tracking-wider">
            Contents
          </p>
          {/* The block is anchored by one short horizontal rule under its
              label. Everything that used to run vertically here — a bar down
              the list, a divider against the text — has gone: beside a page
              already framed by two vertical rules, a third and a fourth stop
              reading as structure and start reading as a fence. */}
          <span aria-hidden className="mt-3 block h-px w-8 bg-surface-brand" />
          <ul className="mt-4 space-y-0.5">
            {items.map((item) => (
              <li key={item.id}>
                <a
                  href={`#${item.id}`}
                  className={cn(
                    'group/toc relative block py-1.5 pl-4 text-meta transition-colors',
                    item.depth > 2 && 'pl-7',
                    activeId === item.id
                      ? 'text-fg'
                      : 'text-fg-subtle hover:text-fg',
                  )}
                >
                  {/* A dot, not a bar. It marks one line without joining the
                      next one into a rule. */}
                  <span
                    aria-hidden
                    className={cn(
                      'absolute left-0 top-[0.95em] size-1.5 rounded-full transition-colors',
                      activeId === item.id
                        ? 'bg-surface-brand'
                        : 'bg-transparent group-hover/toc:bg-fg/25',
                    )}
                  />
                  {item.value}
                </a>
              </li>
            ))}
          </ul>
        </nav>
      )}
    </>
  );
}
