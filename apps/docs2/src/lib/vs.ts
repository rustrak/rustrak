import { mb, num, run, SETUP, times } from './compare';

/**
 * The `/vs/*` pages. Measured figures come from `compare.ts`; the facts in
 * `facts` were checked on each product's own docs and repository on
 * 2026-11-14 (license file, install page, compose file). These pages compare;
 * they do not advertise the other products, so no prices and no links out.
 */

const rustrak = run('rustrak');
const sentry = run('sentry-errors');
const sentryFull = run('sentry');
const glitchtip = run('glitchtip');
const bugsink = run('bugsink');
const bugsinkLite = run('bugsink-sqlite');

export type Stat = {
  value: string;
  label: string;
  /** The two figures the value compares, Rustrak first. */
  others: [string, string][];
};

export type Story = {
  visual: 'fit' | 'howmany' | 'replay' | 'core';
  /** The eyebrow and the section nav's name for it. */
  name: string;
  title: string;
  lead?: string;
  caption: string;
  speed?: number;
};

export type Competitor = {
  slug: string;
  name: string;
  title: string;
  description: string;
  headline: string;
  lead: string;
  /** The run the headline figures compare against. */
  against: string;
  /** Runs in the visuals and the table. */
  runs: string[];
  stats: Stat[];
  /** The page's own visuals, each the one comparison this product loses worst. */
  stories: Story[];
  facts: Record<FactKey, string>;
  faq: { q: string; a: string }[];
};

export const FACT_LABELS = {
  license: 'License',
  language: 'Written in',
  storage: 'Storage',
  minimum: 'Published minimum',
  containers: 'Containers in our run',
} as const;

export type FactKey = keyof typeof FACT_LABELS;

export const RUSTRAK_FACTS: Record<FactKey, string> = {
  license: 'AGPL-3.0',
  language: 'Rust',
  storage: 'SQLite, or PostgreSQL',
  minimum: 'None published',
  containers: '1 (2 with PostgreSQL)',
};

const R = `Rustrak (${rustrak.variant})`;

export const COMPETITORS: Competitor[] = [
  {
    slug: 'sentry',
    name: 'Sentry',
    title: 'Rustrak vs self-hosted Sentry: memory, speed and setup',
    description: `Self-hosted Sentry vs Rustrak on the same 4 cores and 20,000 errors: ${mb(sentry.ram.peak)} vs ${mb(rustrak.ram.peak)} of memory, ${sentry.containers} containers vs 1, ${sentry.storedSecs} s vs ${rustrak.storedSecs} s to store them.`,
    headline: 'Rustrak vs self-hosted Sentry',
    lead: `Both accept events from the official Sentry SDKs. Self-hosted Sentry runs as ${sentry.containers} containers in its smallest profile; Rustrak runs as one. We gave both the same ${num(SETUP.events)} errors on the same four CPU cores.`,
    against: 'sentry-errors',
    runs: ['rustrak', 'rustrak-pg', 'sentry-errors', 'sentry'],
    stats: [
      {
        value: times(sentry.ram.peak, rustrak.ram.peak),
        label: 'less memory at peak',
        others: [
          [R, mb(rustrak.ram.peak)],
          ['Sentry (errors-only)', mb(sentry.ram.peak)],
        ],
      },
      {
        value: `${sentry.containers} to 1`,
        label: 'containers to operate',
        others: [
          [R, '1'],
          ['Sentry (errors-only)', String(sentry.containers)],
        ],
      },
      {
        value: times(sentry.storedSecs, rustrak.storedSecs),
        label: `faster to store ${num(SETUP.events)} errors`,
        others: [
          [R, `${rustrak.storedSecs} s`],
          ['Sentry (errors-only)', `${sentry.storedSecs} s`],
        ],
      },
    ],
    stories: [
      {
        visual: 'fit',
        name: 'Server size',
        title: 'Peak memory against server size',
        lead: `Peak memory under the same burst, as a share of the server it would run on. Sentry's errors-only profile needs a 16 GB server. Rustrak uses less than a tenth of a 1 GB one.`,
        caption:
          "The whole stack's peak memory in our run. Sentry's documentation asks for 16 GB of RAM plus 16 GB of swap.",
      },
      {
        visual: 'howmany',
        name: 'Memory',
        title: `${Math.floor(sentry.ram.peak / rustrak.ram.peak)} Rustrak instances in the memory of one Sentry`,
        caption:
          'Each square is one Rustrak instance at the busiest second of the same run.',
      },
    ],
    facts: {
      license: 'FSL-1.1, Apache-2.0 after two years',
      language: 'Python, with Rust services',
      storage: 'PostgreSQL, ClickHouse, Kafka, Redis, SeaweedFS',
      minimum: '4 cores, 16 GB RAM + 16 GB swap',
      containers: `${sentry.containers} (${sentryFull.containers} with every feature)`,
    },
    faq: [
      {
        q: 'Can Rustrak replace self-hosted Sentry?',
        a: 'For errors, tracing, logs, crons, releases, source maps and user feedback, yes: it accepts the same SDKs, and you switch by changing the DSN. Rustrak has no session replay, profiling, uptime checks or attachments.',
      },
      {
        q: 'How much memory does self-hosted Sentry need?',
        a: `Sentry's documentation asks for 16 GB of RAM plus 16 GB of swap. In our run its errors-only profile peaked at ${mb(sentry.ram.peak)} and the full one at ${mb(sentryFull.ram.peak)}. Rustrak peaked at ${mb(rustrak.ram.peak)} under the same load.`,
      },
      {
        q: 'Do my existing events come with me?',
        a: 'No. Once the SDKs point at Rustrak, it records from the next event on. Keep Sentry running read-only for as long as you need its history.',
      },
      {
        q: 'Is Rustrak made by Sentry?',
        a: 'No. Rustrak implements the protocol the open-source Sentry SDKs speak. Sentry is a trademark of Functional Software, Inc.',
      },
    ],
  },
  {
    slug: 'glitchtip',
    name: 'GlitchTip',
    title: 'Rustrak vs GlitchTip: memory, speed and setup compared',
    description: `GlitchTip 7 vs Rustrak on the same 4 cores and 20,000 errors: ${mb(glitchtip.ram.peak)} vs ${mb(rustrak.ram.peak)} at peak, ${glitchtip.storedSecs} s vs ${rustrak.storedSecs} s to store them, ${glitchtip.cpuMsPerEvent} vs ${rustrak.cpuMsPerEvent} ms of CPU per event.`,
    headline: 'Rustrak vs GlitchTip',
    lead: `Both accept events from the Sentry SDKs and both are lightweight next to Sentry. GlitchTip is a Django application on PostgreSQL and Valkey; Rustrak is a single Rust process on SQLite. We gave both the same ${num(SETUP.events)} errors on the same four CPU cores.`,
    against: 'glitchtip',
    runs: ['rustrak', 'rustrak-pg', 'glitchtip'],
    stats: [
      {
        value: times(glitchtip.ram.peak, rustrak.ram.peak),
        label: 'less memory at peak',
        others: [
          [R, mb(rustrak.ram.peak)],
          ['GlitchTip', mb(glitchtip.ram.peak)],
        ],
      },
      {
        value: times(glitchtip.cpuMsPerEvent, rustrak.cpuMsPerEvent),
        label: 'less CPU per error, databases included',
        others: [
          [R, `${rustrak.cpuMsPerEvent} ms`],
          ['GlitchTip', `${glitchtip.cpuMsPerEvent} ms`],
        ],
      },
      {
        value: times(glitchtip.storedSecs, rustrak.storedSecs),
        label: `faster to store ${num(SETUP.events)} errors`,
        others: [
          [R, `${rustrak.storedSecs} s`],
          ['GlitchTip', `${glitchtip.storedSecs} s`],
        ],
      },
    ],
    stories: [
      {
        visual: 'replay',
        name: 'Time to store',
        title: `Time to store ${num(SETUP.events)} errors`,
        lead: `GlitchTip stored errors as it received them, about ${num(glitchtip.storedPerS)} a second, and had the last one after ${glitchtip.storedSecs} seconds. Rustrak had stored all of them after ${rustrak.storedSecs}.`,
        caption:
          'Replayed at twice the real speed from our measurements: counts every 2 seconds, straight lines between them.',
        speed: 2,
      },
    ],
    facts: {
      license: 'MIT',
      language: 'Python (Django)',
      storage: 'PostgreSQL, Valkey',
      minimum: '256 MB RAM, 512 MB recommended',
      containers: String(glitchtip.containers),
    },
    faq: [
      {
        q: 'Which uses less memory, GlitchTip or Rustrak?',
        a: `Rustrak. Under the same 20,000-error burst it peaked at ${mb(rustrak.ram.peak)}; GlitchTip, with PostgreSQL and Valkey, peaked at ${mb(glitchtip.ram.peak)}.`,
      },
      {
        q: 'Do I need PostgreSQL to run Rustrak?',
        a: 'No. By default Rustrak stores everything in a single SQLite file. PostgreSQL is supported, with its own image.',
      },
      {
        q: 'How do I move from GlitchTip to Rustrak?',
        a: 'Change the DSN in your applications. Both speak the Sentry protocol, so the SDK code stays as it is. Past events remain in GlitchTip.',
      },
    ],
  },
  {
    slug: 'bugsink',
    name: 'Bugsink',
    title: 'Rustrak vs Bugsink: memory, speed and setup compared',
    description: `Bugsink vs Rustrak, both one container on SQLite, same 4 cores and 20,000 errors: ${bugsinkLite.storedSecs} s vs ${rustrak.storedSecs} s to store them, ${bugsinkLite.cpuMsPerEvent} vs ${rustrak.cpuMsPerEvent} ms of CPU per event.`,
    headline: 'Rustrak vs Bugsink',
    lead: 'Both install as a single container on SQLite and accept events from the Sentry SDKs. We ran both that way on the same four CPU cores, then ran Bugsink again on PostgreSQL from its own Compose file.',
    against: 'bugsink-sqlite',
    runs: ['rustrak', 'rustrak-pg', 'bugsink-sqlite', 'bugsink'],
    stats: [
      {
        value: times(bugsinkLite.storedSecs, rustrak.storedSecs),
        label: `faster to store ${num(SETUP.events)} errors, both on SQLite`,
        others: [
          [R, `${rustrak.storedSecs} s`],
          ['Bugsink (SQLite)', `${bugsinkLite.storedSecs} s`],
        ],
      },
      {
        value: times(bugsinkLite.cpuMsPerEvent, rustrak.cpuMsPerEvent),
        label: 'less CPU per error',
        others: [
          [R, `${rustrak.cpuMsPerEvent} ms`],
          ['Bugsink (SQLite)', `${bugsinkLite.cpuMsPerEvent} ms`],
        ],
      },
      {
        value: times(bugsinkLite.ram.peak, rustrak.ram.peak),
        label: 'less memory at peak, one container each',
        others: [
          [R, mb(rustrak.ram.peak)],
          ['Bugsink (SQLite)', mb(bugsinkLite.ram.peak)],
        ],
      },
    ],
    stories: [
      {
        visual: 'core',
        name: 'CPU',
        title: 'Errors stored per second of CPU',
        lead: `One second of CPU divided by what each error cost. Rustrak stores ${num(Math.round(1000 / rustrak.cpuMsPerEvent))} errors with it; Bugsink on the same SQLite, ${num(Math.round(1000 / bugsinkLite.cpuMsPerEvent))}.`,
        caption:
          'From the CPU per error we measured, the whole stack counted. One dot is ten errors.',
      },
      {
        visual: 'replay',
        name: 'Time to store',
        title: `Time to store ${num(SETUP.events)} errors`,
        lead: `On SQLite, Bugsink had stored the last error after ${bugsinkLite.storedSecs} seconds; on PostgreSQL, after ${bugsink.storedSecs}. Rustrak had stored all of them after ${rustrak.storedSecs}.`,
        caption:
          'Replayed at ten times the real speed from our measurements: counts every 2 seconds, straight lines between them.',
        speed: 10,
      },
    ],
    facts: {
      license: 'PolyForm Shield 1.0.0, source-available',
      language: 'Python (Django)',
      storage: 'SQLite, MySQL or PostgreSQL',
      minimum: 'None published',
      containers: '1 (2 with PostgreSQL)',
    },
    faq: [
      {
        q: 'Is Bugsink open source?',
        a: 'Bugsink is published under the PolyForm Shield License 1.0.0, which is source-available and not an OSI-approved open-source license. Rustrak is AGPL-3.0.',
      },
      {
        q: 'Which is faster, Bugsink or Rustrak?',
        a: `Both on SQLite, Rustrak stored a burst of 20,000 errors in ${rustrak.storedSecs} seconds and Bugsink in ${bugsinkLite.storedSecs}, using ${rustrak.cpuMsPerEvent} ms and ${bugsinkLite.cpuMsPerEvent} ms of CPU per error.`,
      },
      {
        q: 'Does Rustrak do more than error tracking?',
        a: 'Yes: tracing, logs, cron monitors, releases with crash-free sessions and user feedback, all from the Sentry SDK you already send errors with.',
      },
    ],
  },
];

/** The FAQ as schema.org `FAQPage`, the same pairs the page shows. */
export function faqJsonLd(items: { q: string; a: string }[]) {
  return {
    '@type': 'FAQPage',
    mainEntity: items.map((item) => ({
      '@type': 'Question',
      name: item.q,
      acceptedAnswer: { '@type': 'Answer', text: item.a },
    })),
  };
}
