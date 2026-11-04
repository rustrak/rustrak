'use client';

import { useLenis } from 'lenis/react';
import * as m from 'motion/react-m';
import { useState } from 'react';
import { formatReleaseDate, type ReleasePulse } from '@/lib/release';
import { cn } from '@/lib/utils';
import { EASE, VIEW } from '../landing/motion';
import { useSmoothScrolling } from '../landing/smooth-scroll';

const TONE: Record<ReleasePulse['channel'], string> = {
  minor: 'bg-surface-brand',
  patch: 'bg-fg/35',
  seed: 'bg-fg/20',
};

/**
 * The whole history on one line: a bar per release, oldest on the left, as
 * tall as the number of changes it carried. Minor releases in lime. Hovering
 * names a release in the readout; clicking goes to it, fetching it first if it
 * is in a part of the archive the page has not loaded.
 */
export function ReleaseStrip({
  pulse,
  onSelect,
}: {
  pulse: ReleasePulse[];
  /** Returns false when the release is already on the page. */
  onSelect: (anchor: string) => boolean;
}) {
  const lenis = useLenis();
  const smooth = useSmoothScrolling();
  const [hover, setHover] = useState<ReleasePulse | null>(null);
  const history = [...pulse].reverse();
  const peak = Math.max(1, ...history.map((release) => release.total));
  const changes = history.reduce((sum, release) => sum + release.total, 0);

  const go = (anchor: string) => {
    if (onSelect(anchor)) return;
    // A button has no href, so the hash a link would have set is written here.
    window.history.pushState(null, '', `#${anchor}`);
    if (lenis && smooth)
      lenis.scrollTo(`#${anchor}`, { offset: -96, duration: 1.6 });
    else
      document.getElementById(anchor)?.scrollIntoView({ behavior: 'smooth' });
  };

  let year = '';
  const latest = pulse[0];
  const stats = [
    { label: 'Releases', value: history.length },
    { label: 'Changes', value: changes },
    { label: 'Latest', value: latest?.version ?? '—' },
  ];

  return (
    <div
      onPointerLeave={() => setHover(null)}
      className="rounded-3xl border border-border-subtle bg-panel p-6 sm:p-8"
    >
      <dl className="mb-10 grid grid-cols-3 gap-6">
        {stats.map((stat) => (
          <div key={stat.label} className="flex flex-col gap-1.5">
            <dd className="order-1 text-fg text-page-title tabular-nums">
              {stat.value}
            </dd>
            <dt className="order-2 text-fg-meta text-meta">{stat.label}</dt>
          </div>
        ))}
      </dl>
      <div className="flex h-20 items-end gap-[2px] sm:h-28 sm:gap-[3px]">
        {history.map((release, i) => {
          const active = hover === release;
          return (
            <button
              key={release.anchor}
              type="button"
              aria-label={`${release.version}: ${release.title}`}
              onPointerEnter={() => setHover(release)}
              onFocus={() => setHover(release)}
              onBlur={() => setHover(null)}
              onClick={() => go(release.anchor)}
              className="flex h-full min-w-0 flex-1 cursor-pointer items-end"
            >
              <m.span
                className={cn(
                  'block w-full origin-bottom rounded-xs transition-opacity duration-300',
                  TONE[release.channel],
                  hover && !active && 'opacity-35',
                )}
                style={{
                  height: `${Math.max(6, (release.total / peak) * 100)}%`,
                }}
                initial={{ scaleY: 0 }}
                whileInView={{ scaleY: 1 }}
                viewport={VIEW}
                transition={{ duration: 1, ease: EASE, delay: 0.3 + i * 0.015 }}
              />
            </button>
          );
        })}
      </div>

      {/* Years under the first release of each, on the same flex track so
          they line up with the bars above. */}
      <div aria-hidden className="mt-3 flex gap-[2px] sm:gap-[3px]">
        {history.map((release) => {
          const starts = release.date.slice(0, 4) !== year;
          year = release.date.slice(0, 4);
          return (
            <span
              key={release.anchor}
              className="min-w-0 flex-1 overflow-visible whitespace-nowrap font-mono text-fg-meta text-mono-sm"
            >
              {starts ? year : null}
            </span>
          );
        })}
      </div>

      <p className="mt-5 h-5 truncate text-fg-meta text-meta">
        {hover ? (
          <>
            <span className="text-fg">{hover.version}</span>
            <span className="mx-3 opacity-50">/</span>
            {hover.title}
            <span className="mx-3 hidden opacity-50 sm:inline">/</span>
            <span className="hidden sm:inline">
              {formatReleaseDate(hover.date)}
            </span>
            <span className="mx-3 opacity-50">/</span>
            {hover.total} changes
          </>
        ) : (
          'One bar per release, as tall as its changes. Lime is a minor.'
        )}
      </p>
    </div>
  );
}
