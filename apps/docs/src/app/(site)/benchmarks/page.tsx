import type { Metadata } from 'next';
import Link from 'next/link';
import type { ReactNode } from 'react';
import {
  Figure,
  Heading,
  ResultsTable,
  Stat,
} from '@/components/compare/figures';
import {
  MemoryChart,
  RaceChart,
  RankBars,
} from '@/components/compare/lazy-charts';
import { Backdrop, PageIntro } from '@/components/editorial/intro';
import { Shell } from '@/components/editorial/shell';
import { Reveal } from '@/components/home/reveal';
import { CONTAINER } from '@/components/home/site';
import { HARNESS, MEASURED, mb, num, RUNS, run, SETUP } from '@/lib/compare';
import { baseMetadata, jsonLd, SITE } from '@/lib/seo';

const title = 'Benchmark: Rustrak vs Sentry, GlitchTip and Bugsink';
const description =
  'Rustrak, self-hosted Sentry, GlitchTip and Bugsink on the same machine and the same burst of 20,000 errors: time to store them, memory, CPU per event and disk.';

export const metadata: Metadata = {
  title: { absolute: title },
  description,
  alternates: { canonical: '/benchmarks' },
  openGraph: {
    ...baseMetadata.openGraph,
    title,
    description,
    url: '/benchmarks',
  },
  twitter: { ...baseMetadata.twitter, title, description },
};

const rustrak = run('rustrak');
const rustrakPg = run('rustrak-pg');
const glitchtip = run('glitchtip');
const bugsink = run('bugsink');
const bugsinkLite = run('bugsink-sqlite');
const sentry = run('sentry-errors');
const sentryFull = run('sentry');

/** The products whose memory fits one scale; Sentry would flatten them. */
const SMALL = RUNS.filter((r) => !r.id.startsWith('sentry')).map((r) => r.id);

const METHOD: [string, ReactNode][] = [
  ['Machine', `${SETUP.host}. One product at a time, each from empty volumes.`],
  [
    'Cores',
    `Every stack was pinned to the same ${SETUP.cores} cores. The load generator and the sampler ran on the other eight, so neither took CPU from the product.`,
  ],
  [
    'Load',
    `${num(SETUP.events)} gzipped Sentry envelopes of about ${SETUP.eventKb} KB, ${SETUP.concurrency} in flight, spread over ${num(SETUP.groups)} different issues, posted to each product's envelope endpoint with a real DSN.`,
  ],
  [
    'Memory',
    <>
      The cgroup <code className="font-mono text-fg">anon</code> counter of
      every container in the stack, added up once a second. That is memory the
      processes allocated, without the page cache the kernel can take back.
    </>,
  ],
  [
    'CPU',
    'CPU time of the whole stack from the first request to the last event stored, divided by the number of events.',
  ],
  [
    'Stored',
    "A count of rows in each product's own events table (ClickHouse for Sentry), every 2 seconds once the burst ended. An event counts when a query can return it, not when the endpoint answers 200.",
  ],
  ['Disk', "The size of the stack's Docker volumes right after the run."],
  [
    'Configuration',
    "Each product's own Docker Compose file. We changed only what would have dropped events: rate limits, raised for everyone, and Bugsink's per-project limit of 10,000 stored events.",
  ],
];

const CAVEATS: ReactNode[] = [
  `Sentry answers first. Relay accepted ${num(sentry.acceptPerS)} events a second because it replies as soon as an event is on Kafka. Writing them to ClickHouse is what took ${sentry.storedSecs} seconds.`,
  'Sentry asks for 16 GB of swap and Docker Desktop gives its VM 1 GB. The feature-complete profile ran close to the edge, and in an earlier attempt ClickHouse was killed for lack of memory. Read the errors-only column as the fair one.',
  `Rustrak left more on disk with SQLite (${mb(rustrak.diskMb)}) than with PostgreSQL (${mb(rustrakPg.diskMb)}). That is the volume size right after the run; we did not dig into why.`,
  'One run, in a VM on a Mac. Your server will give different absolute numbers. The distance between products is what carries over.',
  'Only ingestion was measured: no search, no dashboard, no features. Sentry does far more than the other three, and pays for it here.',
];

const structuredData = {
  '@context': 'https://schema.org',
  '@graph': [
    {
      '@type': 'TechArticle',
      headline: title,
      description,
      url: `${SITE}/benchmarks`,
      datePublished: MEASURED,
      dateModified: MEASURED,
      author: { '@id': `${SITE}/#organization` },
      publisher: { '@id': `${SITE}/#organization` },
    },
    {
      '@type': 'Dataset',
      name: 'Self-hosted error tracker ingestion benchmark',
      description,
      url: `${SITE}/benchmarks`,
      datePublished: MEASURED,
      creator: { '@type': 'Organization', name: 'Rustrak', url: SITE },
      license: 'https://www.gnu.org/licenses/agpl-3.0.html',
      measurementTechnique:
        'cgroup v2 memory and CPU accounting under a fixed burst of Sentry envelopes',
      variableMeasured: [
        'Seconds to store 20,000 events',
        'Memory (MB)',
        'CPU milliseconds per event',
        'Events accepted per second',
        'Disk (MB)',
      ],
    },
  ],
};

const SECTION = `${CONTAINER} py-16 sm:py-24`;

export default function Page() {
  return (
    <Shell>
      <script {...jsonLd(structuredData)} />
      <PageIntro
        eyebrow="Benchmarks"
        title="We sent 20,000 errors to four trackers."
        lead="Same machine, same four cores, same burst. Then we timed how long each one took to store it all, and watched the memory and CPU it spent doing it."
        meta={
          <>
            <span>Measured {MEASURED}</span>
            <span aria-hidden>·</span>
            <a href={HARNESS} className="transition-colors hover:text-fg">
              The script, to run it yourself ↗
            </a>
          </>
        }
      />

      <section className={`${CONTAINER} pt-16 sm:pt-24`}>
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          <Stat
            value={`${rustrak.storedSecs} s`}
            label={`until Rustrak had all ${num(SETUP.events)} stored. GlitchTip took ${glitchtip.storedSecs} s, Sentry ${sentry.storedSecs} s, Bugsink ${bugsinkLite.storedSecs} s on SQLite and ${bugsink.storedSecs} s on PostgreSQL.`}
          />
          <Stat
            delay={0.06}
            value={mb(rustrak.ram.peak)}
            label={`of memory at its busiest. GlitchTip reached ${mb(glitchtip.ram.peak)}, Sentry ${mb(sentry.ram.peak)}.`}
          />
          <Stat
            delay={0.12}
            value={`${rustrak.cpuMsPerEvent} ms`}
            label={`of CPU per event, database included. Sentry spent ${sentry.cpuMsPerEvent} ms.`}
          />
          <Stat
            delay={0.18}
            value="1"
            label={`container. Sentry's smallest profile starts ${sentry.containers}.`}
          />
        </div>
      </section>

      <section className={SECTION}>
        <Heading
          id="race"
          title="The race to 20,000"
          lead={`Each line counts the events a query could find, from the first request on. Rustrak had them all ${rustrak.storedSecs} seconds in. Sentry took the burst in two seconds and spent the next ${sentry.storedSecs - 2} writing it down.`}
        />
        <div className="mt-10">
          <Figure caption="Time runs on a log scale, so a 6-second finish and a 280-second one fit on one chart. Counted every 2 seconds after the burst ended; before the first count the line is drawn straight. Hover for every product at any second.">
            <RaceChart />
          </Figure>
        </div>
      </section>

      <section className={SECTION}>
        <Heading
          id="memory"
          title="Memory while it works"
          lead={`The whole stack, sampled every second: server, database, queues. Rustrak on SQLite sat at ${mb(rustrak.ram.idle)} before the burst and topped out at ${mb(rustrak.ram.peak)}. GlitchTip went from ${mb(glitchtip.ram.idle)} to ${mb(glitchtip.ram.peak)}.`}
        />
        <div className="mt-10 grid gap-4">
          <Figure
            caption={`Sentry is left off this chart. It idled at ${mb(sentry.ram.idle)} before the first event arrived, and on the same scale every other line would sit flat on the floor.`}
          >
            <MemoryChart ids={SMALL} />
          </Figure>
          <div className="grid items-start gap-4">
            <Figure caption="Peak memory, all on one scale. Hover a bar for its value.">
              <h3 className="text-fg text-section">Peak memory</h3>
              <RankBars metric="peak" unit="" />
            </Figure>
          </div>
        </div>
      </section>

      <section className={SECTION}>
        <div className="grid items-start gap-10 lg:grid-cols-[2fr_3fr]">
          <Heading
            id="cpu"
            title="CPU per event"
            lead={`All the CPU the stack burned between the first request and the last event stored, divided by ${num(SETUP.events)}. On a two-core VPS this is the number that decides how big a spike you can take.`}
          />
          <Figure>
            <RankBars metric="cpu" unit="ms" />
          </Figure>
        </div>
      </section>

      <section className={SECTION}>
        <Heading
          id="results"
          title="Every number"
          lead={`Rustrak on its default SQLite and on PostgreSQL, Sentry with its errors-only profile and with everything on (${sentryFull.containers} containers). All of them stored every event.`}
        />
        <div className="mt-10">
          <ResultsTable runs={RUNS} />
        </div>
      </section>

      <section className={SECTION}>
        <Heading id="caveats" title="What this does not tell you" />
        <ul className="mt-10 grid gap-x-10 gap-y-6 md:grid-cols-2">
          {CAVEATS.map((text, i) => (
            <Reveal
              as="li"
              // Fixed list: the position is the identity.
              // react-doctor-disable-next-line react-doctor/no-array-index-as-key
              key={i}
              delay={i * 0.04}
              className="border-border-subtle border-l-2 pl-5 text-pretty text-fg-secondary text-value"
            >
              {text}
            </Reveal>
          ))}
        </ul>
      </section>

      <section className={SECTION}>
        <Heading id="method" title="How it was measured" />
        <dl className="mt-10 grid gap-x-10 gap-y-8 md:grid-cols-2">
          {METHOD.map(([term, text]) => (
            <Reveal key={term} className="flex flex-col gap-2">
              <dt className="font-mono text-fg-brand text-mono uppercase tracking-wider">
                {term}
              </dt>
              <dd className="text-pretty text-fg-secondary text-value">
                {text}
              </dd>
            </Reveal>
          ))}
        </dl>
        <Reveal className="mt-12 overflow-x-auto rounded-2xl border border-border-subtle bg-panel p-5 font-mono text-fg-secondary text-mono">
          <pre>
            <span className="text-fg-meta">
              # Docker, and about 16 GB free for Sentry
            </span>
            {'\n'}packages/benchmarks/compare/run.sh rustrak rustrak-pg
            glitchtip bugsink bugsink-sqlite sentry-errors sentry
          </pre>
        </Reveal>
      </section>

      <section className={SECTION}>
        <Reveal y={48}>
          <Backdrop dim>
            <div className="flex flex-col gap-8 px-6 py-16 sm:px-12 sm:py-20">
              <h2 className="max-w-xl text-balance text-fg text-headline">
                Side by side, one at a time
              </h2>
              <ul className="grid gap-4 md:grid-cols-3">
                {[
                  ['sentry', 'Rustrak vs self-hosted Sentry'],
                  ['glitchtip', 'Rustrak vs GlitchTip'],
                  ['bugsink', 'Rustrak vs Bugsink'],
                ].map(([slug, text]) => (
                  <li key={slug}>
                    <Link
                      href={`/vs/${slug}`}
                      className="flex h-full items-center justify-between gap-4 rounded-2xl border border-border-subtle bg-canvas/60 p-6 text-fg text-section transition-colors hover:border-border"
                    >
                      {text}
                      <span aria-hidden className="text-fg-brand">
                        →
                      </span>
                    </Link>
                  </li>
                ))}
              </ul>
            </div>
          </Backdrop>
        </Reveal>
      </section>
    </Shell>
  );
}
