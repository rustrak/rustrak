'use client';

import { cn } from '@rustrak/ui';
import * as m from 'motion/react-m';
import type { ReactNode } from 'react';
import { EASE, VIEW } from '../landing/motion';
import { SectionHead } from './chrome';
import { Reveal } from './reveal';
import { CONTAINER } from './site';

/**
 * Facts only, each checkable:
 * - Self-hosted Sentry's requirements: develop.sentry.dev/self-hosted
 *   ("4 CPU Cores, 16 GB RAM + 16 GB swap, 20 GB Free Disk Space").
 * - 53 services: the `services:` keys in getsentry/self-hosted's
 *   docker-compose.yml (master, Nov 2026). Recount when it changes.
 * - Sentry's Team plan: $26/mo billed annually, sentry.io/pricing.
 * - Rustrak's memory: README's 20,000-event burst on 4 cores with SQLite.
 */
const SENTRY_SERVICES = 53;

const COLUMNS = ['Rustrak', 'Self-hosted Sentry', 'Sentry cloud'] as const;

const ROWS: { label: string; values: [string, string, string] }[] = [
  {
    label: 'Where your data lives',
    values: ['Your server', 'Your server', "Sentry's cloud"],
  },
  {
    label: 'Storage',
    values: [
      'SQLite or PostgreSQL',
      'PostgreSQL, ClickHouse, Kafka, Redis',
      'Managed',
    ],
  },
  {
    label: 'Price',
    values: [
      'Free, AGPL-3.0',
      'Free, plus the hardware',
      'From $26/mo, by volume',
    ],
  },
  {
    label: 'Works with the Sentry SDKs',
    values: ['Yes', 'Yes', 'Yes'],
  },
];

function Card({
  title,
  caption,
  children,
  delay = 0,
}: {
  title: string;
  caption: string;
  children: ReactNode;
  delay?: number;
}) {
  return (
    <Reveal
      y={40}
      delay={delay}
      className="flex flex-col gap-6 rounded-3xl border border-border-subtle bg-panel p-6 sm:p-8"
    >
      <div className="flex flex-col gap-1">
        <h3 className="text-fg text-section">{title}</h3>
        <p className="text-fg-subtle text-meta">{caption}</p>
      </div>
      {children}
    </Reveal>
  );
}

/** Grows to its width as it scrolls in, after its card has settled. */
const GROW = { duration: 1.6, ease: EASE, delay: 0.5 } as const;

/** One horizontal bar, to scale against `max`; never thinner than a sliver. */
function Bar({
  label,
  value,
  amount,
  max,
  brand,
  hatched,
}: {
  label: string;
  value: string;
  amount: number;
  max: number;
  brand?: boolean;
  /** A second stretch drawn hatched: swap, beside the RAM. */
  hatched?: number;
}) {
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-baseline justify-between gap-4">
        <span
          className={cn('text-value', brand ? 'text-fg' : 'text-fg-subtle')}
        >
          {label}
        </span>
        <span
          className={cn(
            'font-mono text-mono tabular-nums',
            brand ? 'text-fg-brand' : 'text-fg-secondary',
          )}
        >
          {value}
        </span>
      </div>
      <div className="h-3 overflow-hidden rounded-pill bg-surface">
        {/* Drawn at full size and uncovered left to right: a clip animates
            without relayout, where a width would reflow every frame. */}
        <m.div
          className="flex h-full"
          initial={{ clipPath: 'inset(0 100% 0 0)' }}
          whileInView={{ clipPath: 'inset(0 0% 0 0)' }}
          viewport={VIEW}
          transition={GROW}
        >
          <span
            className={cn(
              'h-full min-w-1',
              brand ? 'rounded-pill bg-surface-brand' : 'bg-fg-ghost',
            )}
            style={{ width: `${(amount / max) * 100}%` }}
          />
          {hatched ? (
            <span
              className="h-full bg-[repeating-linear-gradient(135deg,var(--fg-ghost)_0_2px,transparent_2px_6px)]"
              style={{ width: `${(hatched / max) * 100}%` }}
            />
          ) : null}
        </m.div>
      </div>
    </div>
  );
}

/** Each square is one container the stack needs running. */
function Units({ count, brand }: { count: number; brand?: boolean }) {
  return (
    <div className="flex flex-wrap gap-1.5">
      {Array.from({ length: count }, (_, index) => (
        <m.span
          initial={{ opacity: 0, scale: 0.4 }}
          whileInView={{ opacity: 1, scale: 1 }}
          viewport={VIEW}
          transition={{ duration: 0.6, ease: EASE, delay: 0.5 + index * 0.012 }}
          // Identical squares: the position is the identity.
          // react-doctor-disable-next-line react-doctor/no-array-index-as-key
          key={index}
          className={cn(
            'size-3.5 rounded-xs',
            brand ? 'bg-surface-brand' : 'bg-fg-ghost',
          )}
        />
      ))}
    </div>
  );
}

export function Compare() {
  return (
    <section className="py-16 sm:py-24">
      <div className={CONTAINER}>
        <SectionHead
          eyebrow="Compare"
          title="The same SDKs, a fraction of the machine."
          lead="Sentry is a platform, and self-hosting it means running that platform. Rustrak is the part most teams use every day, built to run on the machine you already have."
        />

        <div className="mt-12 grid gap-4 lg:grid-cols-2">
          <Card
            title="Memory"
            caption="What each needs to keep in RAM, drawn to the same scale."
          >
            <div className="flex flex-col gap-5">
              <Bar
                label="Self-hosted Sentry, minimum"
                value="16 GB + 16 GB swap"
                amount={16_384}
                hatched={16_384}
                max={32_768}
              />
              <Bar
                label="Rustrak, peak under a 20,000-event burst"
                value="< 60 MB"
                amount={60}
                max={32_768}
                brand
              />
            </div>
          </Card>

          <Card
            delay={0.12}
            title="Services to run"
            caption="Every square is a container that has to be up, patched and backed up."
          >
            <div className="flex flex-col gap-5">
              <div className="flex flex-col gap-2.5">
                <div className="flex items-baseline justify-between">
                  <span className="text-fg-subtle text-value">
                    Self-hosted Sentry
                  </span>
                  <span className="font-mono text-fg-secondary text-mono tabular-nums">
                    {SENTRY_SERVICES}
                  </span>
                </div>
                <Units count={SENTRY_SERVICES} />
              </div>
              <div className="flex flex-col gap-2.5">
                <div className="flex items-baseline justify-between">
                  <span className="text-fg text-value">Rustrak</span>
                  <span className="font-mono text-fg-brand text-mono tabular-nums">
                    1
                  </span>
                </div>
                <Units count={1} brand />
              </div>
            </div>
          </Card>
        </div>

        <Reveal
          y={40}
          className="mt-4 overflow-x-auto rounded-3xl border border-border-subtle"
        >
          <table className="w-full min-w-[44rem] border-collapse text-start">
            <thead>
              <tr className="border-border-subtle border-b">
                <th className="w-[28%] p-5" />
                {COLUMNS.map((column, index) => (
                  <th
                    key={column}
                    scope="col"
                    className={cn(
                      'p-5 text-start text-section',
                      index === 0 ? 'bg-surface text-fg-brand' : 'text-fg',
                    )}
                  >
                    {column}
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
                  <th
                    scope="row"
                    className="p-5 text-start font-normal text-fg-meta text-value"
                  >
                    {row.label}
                  </th>
                  {row.values.map((value, index) => (
                    <td
                      key={COLUMNS[index]}
                      className={cn(
                        'p-5 text-value',
                        index === 0 ? 'bg-surface text-fg' : 'text-fg-subtle',
                      )}
                    >
                      {value}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </Reveal>
        <p className="mt-4 text-fg-meta text-meta">
          Sentry figures from its self-hosted documentation, docker-compose.yml
          and pricing page. Rustrak measured on a 4-core machine with SQLite.
        </p>
      </div>
    </section>
  );
}
