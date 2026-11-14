'use client';

import { useInView } from 'motion/react';
import * as m from 'motion/react-m';
import { useEffect, useRef, useState } from 'react';
import { label, mb, num, RUNS, type Run, run, SETUP } from '@/lib/compare';
import SERIES from '@/lib/compare-series.json';
import { cn } from '@/lib/utils';
import { EASE, VIEW } from '../landing/motion';

const series = SERIES as Record<string, { stored: number[][] }>;

/** Each product's stored-count line, cut where it reached the last event. */
function lineOf(id: string) {
  const points = series[id].stored;
  const done = points.findIndex(([, n]) => n === points.at(-1)?.[1]);
  return points.slice(0, done + 1);
}

/** Events stored `t` seconds in: measured counts, straight between them. */
function storedAt(line: number[][], t: number) {
  const last = line.at(-1) ?? [0, 0];
  if (t >= last[0]) return last[1];
  const i = line.findIndex(([pt]) => pt >= t);
  if (i <= 0) return 0;
  const [t0, n0] = line[i - 1];
  const [t1, n1] = line[i];
  return n0 + ((n1 - n0) * (t - t0)) / (t1 - t0);
}

const ink = (r: Run) => (r.rustrak ? 'text-fg' : 'text-fg-subtle');

/* ── GlitchTip: the burst, replayed ─────────────────────────────────────── */

/**
 * The race at real speed, one lane per product: the bar is the share of the
 * 20,000 events a query could find at that second. Starts when it scrolls in.
 */
export function Replay({ ids, speed = 1 }: { ids: string[]; speed?: number }) {
  // A product whose run was incomplete has no series: it is left out.
  const runs = RUNS.filter((r) => ids.includes(r.id) && series[r.id]);
  const lines = Object.fromEntries(runs.map((r) => [r.id, lineOf(r.id)]));
  const end = Math.max(...runs.map((r) => r.storedSecs));

  const ref = useRef<HTMLDivElement>(null);
  const seen = useInView(ref, { once: true, margin: '-20% 0px' });
  const [t, setT] = useState(0);
  const [round, setRound] = useState(0);

  useEffect(() => {
    if (!seen) return;
    let frame = 0;
    const start = performance.now();
    const tick = (now: number) => {
      const next = Math.min(((now - start) / 1000) * speed, end);
      setT(next);
      if (next < end) frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [seen, round, speed, end]);

  return (
    <div ref={ref} className="flex flex-col gap-8">
      <div className="flex items-end justify-between gap-6">
        <div className="flex flex-col gap-1">
          <span className="font-mono text-fg-meta text-mono uppercase tracking-wider">
            Since the first request{speed > 1 ? ` · ${speed}× speed` : ''}
          </span>
          <span className="text-fg text-headline tabular-nums">
            {t.toFixed(1)} s
          </span>
        </div>
        <button
          type="button"
          onClick={() => {
            setT(0);
            setRound((n) => n + 1);
          }}
          className="rounded-pill border border-border-subtle px-4 py-2 text-control text-fg-subtle transition-colors hover:text-fg"
        >
          Replay
        </button>
      </div>
      <ul className="flex flex-col gap-6">
        {runs.map((r) => {
          const n = storedAt(lines[r.id], t);
          const done = t >= r.storedSecs;
          return (
            <li key={r.id} className="flex flex-col gap-2.5">
              <div className="flex items-baseline justify-between gap-4">
                <span className={cn('text-value', ink(r))}>{label(r)}</span>
                <span
                  className={cn(
                    'font-mono text-mono tabular-nums',
                    done && r.rustrak ? 'text-fg-brand' : 'text-fg-secondary',
                  )}
                >
                  {done
                    ? `all ${num(SETUP.events)} in ${r.storedSecs} s`
                    : `${num(Math.round(n))} stored`}
                </span>
              </div>
              <div className="h-3 overflow-hidden rounded-pill bg-surface">
                <div
                  className={cn(
                    'h-full rounded-pill',
                    r.rustrak ? 'bg-surface-brand' : 'bg-fg-ghost',
                  )}
                  style={{ width: `${(n / SETUP.events) * 100}%` }}
                />
              </div>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

/* ── Sentry: what fits on your server ───────────────────────────────────── */

const SIZES = [1, 2, 4, 8, 16];

/** Names short enough for a fifth of the page. */
const SHORT: Record<string, string> = {
  rustrak: 'Rustrak',
  'sentry-errors': 'Sentry, errors',
  sentry: 'Sentry, full',
};

/** One machine size, each product's peak memory as a share of it. */
export function ServerFit({ ids }: { ids: string[] }) {
  const runs = RUNS.filter((r) => ids.includes(r.id));
  return (
    <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-5">
      {SIZES.map((gb, i) => (
        <m.div
          key={gb}
          initial={{ opacity: 0, y: 24 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={VIEW}
          transition={{ duration: 0.9, ease: EASE, delay: i * 0.08 }}
          className="flex flex-col gap-5 rounded-2xl border border-border-subtle bg-canvas/40 p-5"
        >
          <span className="text-fg text-title">{gb} GB server</span>
          <ul className="flex flex-col gap-4">
            {runs.map((r) => {
              const share = r.ram.peak / (gb * 1024);
              const fits = share <= 1;
              return (
                <li key={r.id} className="flex flex-col gap-1.5">
                  <div className="flex items-baseline justify-between gap-2 text-meta">
                    <span className={ink(r)}>{SHORT[r.id] ?? label(r)}</span>
                    <span
                      className={cn(
                        'font-mono tabular-nums',
                        fits ? 'text-fg-secondary' : 'text-fg-meta',
                      )}
                    >
                      {fits
                        ? `${Math.max(1, Math.round(share * 100))}%`
                        : 'too big'}
                    </span>
                  </div>
                  <div className="h-2 overflow-hidden rounded-pill bg-surface">
                    <m.div
                      className={cn(
                        'h-full',
                        r.rustrak
                          ? 'rounded-pill bg-surface-brand'
                          : fits
                            ? 'bg-fg-ghost'
                            : 'bg-[repeating-linear-gradient(135deg,var(--fg-ghost)_0_2px,transparent_2px_6px)]',
                      )}
                      // Drawn at its width and grown from the left: a scale
                      // animates without relayout.
                      style={{
                        width: `${Math.max(Math.min(share, 1) * 100, 1.5)}%`,
                        transformOrigin: 'left',
                      }}
                      initial={{ scaleX: 0 }}
                      whileInView={{ scaleX: 1 }}
                      viewport={VIEW}
                      transition={{
                        duration: 1.4,
                        ease: EASE,
                        delay: 0.3 + i * 0.08,
                      }}
                    />
                  </div>
                </li>
              );
            })}
          </ul>
        </m.div>
      ))}
    </div>
  );
}

/** How many Rustraks at their peak fit in the memory one Sentry used. */
export function HowMany({ other }: { other: string }) {
  const rustrak = run('rustrak');
  const them = run(other);
  const count = Math.floor(them.ram.peak / rustrak.ram.peak);
  return (
    <div className="flex flex-col gap-6">
      <p className="max-w-2xl text-pretty text-fg-secondary text-lead">
        <span className="text-fg-brand">{count} Rustraks</span>, each at its
        busiest ({mb(rustrak.ram.peak)}), fit in the {mb(them.ram.peak)} that{' '}
        {label(them)} used for the same burst.
      </p>
      <div className="flex flex-col gap-3 rounded-2xl border border-border-subtle bg-surface/40 p-5">
        <div className="flex items-baseline justify-between gap-4 text-meta">
          <span className="text-fg-subtle">{label(them)}, at its busiest</span>
          <span className="font-mono text-fg-secondary tabular-nums">
            {mb(them.ram.peak)}
          </span>
        </div>
        <div className="flex flex-wrap gap-1.5" aria-hidden>
          {Array.from({ length: count }, (_, i) => (
            <m.span
              // Identical squares: the position is the identity.
              // react-doctor-disable-next-line react-doctor/no-array-index-as-key
              key={i}
              className="size-3.5 rounded-xs bg-surface-brand"
              initial={{ opacity: 0, scale: 0.3 }}
              whileInView={{ opacity: 1, scale: 1 }}
              viewport={VIEW}
              transition={{ duration: 0.5, ease: EASE, delay: 0.2 + i * 0.015 }}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

/* ── Bugsink: what one CPU core buys ────────────────────────────────────── */

const PER_DOT = 10;

/**
 * Events stored per second of CPU, from the measured CPU per event: what one
 * fully busy core would get through. One dot is ten events.
 */
export function CoreBudget({ ids }: { ids: string[] }) {
  const runs = RUNS.filter((r) => ids.includes(r.id));
  return (
    <ul className="flex flex-col gap-8">
      {runs.map((r) => {
        const perSecond = 1000 / r.cpuMsPerEvent;
        const dots = Math.max(1, Math.round(perSecond / PER_DOT));
        return (
          <li key={r.id} className="flex flex-col gap-3">
            <div className="flex items-baseline justify-between gap-4">
              <span className={cn('text-value', ink(r))}>{label(r)}</span>
              <span
                className={cn(
                  'font-mono text-mono tabular-nums',
                  r.rustrak ? 'text-fg-brand' : 'text-fg-secondary',
                )}
              >
                {num(Math.round(perSecond))} events per CPU-second
              </span>
            </div>
            <div className="flex flex-wrap gap-1" aria-hidden>
              {Array.from({ length: dots }, (_, i) => (
                <m.span
                  // Identical dots: the position is the identity.
                  // react-doctor-disable-next-line react-doctor/no-array-index-as-key
                  key={i}
                  className={cn(
                    'size-2 rounded-full',
                    r.rustrak ? 'bg-surface-brand' : 'bg-fg-ghost',
                  )}
                  initial={{ opacity: 0 }}
                  whileInView={{ opacity: 1 }}
                  viewport={VIEW}
                  transition={{ duration: 0.3, delay: 0.2 + i * 0.006 }}
                />
              ))}
            </div>
          </li>
        );
      })}
    </ul>
  );
}
