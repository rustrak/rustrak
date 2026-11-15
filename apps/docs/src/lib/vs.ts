import { mb, num, run, SETUP, times } from './compare';

/**
 * The `/vs/*` pages. Measured figures come from `compare.ts`; the facts in
 * `facts` were checked on each product's own docs and repository on
 * 2026-11-14 (license file, install page, compose file). These pages compare;
 * they do not advertise the other products, so no prices and no links out.
 */

const rustrak = run('rustrak');

export type Competitor = {
  slug: string;
  name: string;
  title: string;
  description: string;
  headline: string;
  lead: string;
  /** Runs in the race, CPU and table. */
  runs: string[];
  stats: { value: string; label: string }[];
  /** The page's own visuals, each the one comparison this product loses worst. */
  stories: Story[];
  facts: Record<FactKey, string>;
  faq: { q: string; a: string }[];
};

export type Story = {
  visual: 'fit' | 'howmany' | 'replay' | 'core';
  title: string;
  lead?: string;
  caption: string;
  ids?: string[];
  speed?: number;
};

export const FACT_LABELS = {
  license: 'License',
  language: 'Written in',
  storage: 'What it stores into',
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

const sentry = run('sentry-errors');
const sentryFull = run('sentry');
const glitchtip = run('glitchtip');
const bugsink = run('bugsink');
const bugsinkLite = run('bugsink-sqlite');

export const COMPETITORS: Competitor[] = [
  {
    slug: 'sentry',
    name: 'Sentry',
    title: 'Rustrak vs self-hosted Sentry: memory, speed and setup',
    description: `Self-hosted Sentry vs Rustrak on the same 4 cores and 20,000 errors: ${mb(sentry.ram.peak)} vs ${mb(rustrak.ram.peak)} of memory, ${sentry.containers} containers vs 1, ${sentry.storedSecs} s vs ${rustrak.storedSecs} s to store them.`,
    headline: 'Rustrak vs self-hosted Sentry',
    lead: `Both take events from the official Sentry SDKs. Self-hosted Sentry is ${sentry.containers} containers before you switch everything on; Rustrak is one. We put them on the same four cores and sent both the same ${num(SETUP.events)} errors.`,
    runs: ['rustrak', 'rustrak-pg', 'sentry-errors', 'sentry'],
    stats: [
      {
        value: times(sentry.ram.peak, rustrak.ram.peak),
        label: `less memory. ${mb(rustrak.ram.peak)} for Rustrak at its busiest, ${mb(sentry.ram.peak)} for Sentry's smallest profile.`,
      },
      {
        value: `1 vs ${sentry.containers}`,
        label: `containers to keep running. ${sentryFull.containers} with every Sentry feature on.`,
      },
      {
        value: `${rustrak.storedSecs} s`,
        label: `for Rustrak to store all ${num(SETUP.events)}. Sentry needed ${sentry.storedSecs} s.`,
      },
    ],
    stories: [
      {
        visual: 'fit',
        title: 'Where each one fits',
        lead: `Peak memory under the same burst, against the size of the server you would rent. Sentry's errors-only profile needs the 16 GB box. Rustrak leaves more than nine tenths of a 1 GB one free.`,
        caption:
          "The whole stack's peak memory in our run. Sentry's documentation asks for 16 GB of RAM plus 16 GB of swap.",
      },
      {
        visual: 'howmany',
        title: 'One Sentry, a hundred Rustraks',
        caption:
          'Each square is one Rustrak at the busiest second of the same run.',
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
        a: 'For errors, tracing, logs, crons, releases, source maps and user feedback, yes: it takes the same SDKs and you switch by changing the DSN. Rustrak has no session replay, profiling, uptime checks or attachments.',
      },
      {
        q: 'How much memory does self-hosted Sentry need?',
        a: `Sentry's documentation asks for 16 GB of RAM plus 16 GB of swap. In our run its errors-only profile peaked at ${mb(sentry.ram.peak)} and the full one at ${mb(sentryFull.ram.peak)}. Rustrak peaked at ${mb(rustrak.ram.peak)} under the same load.`,
      },
      {
        q: 'Do my old events come with me?',
        a: 'No. Point the SDKs at Rustrak and it starts from the next event. Keep Sentry running read-only for as long as you need the history.',
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
    lead: `Both take events from the Sentry SDKs and both are small next to Sentry. GlitchTip is a Django app on PostgreSQL and Valkey; Rustrak is one Rust process on SQLite. Same four cores, same ${num(SETUP.events)} errors.`,
    runs: ['rustrak', 'rustrak-pg', 'glitchtip'],
    stats: [
      {
        value: times(glitchtip.ram.peak, rustrak.ram.peak),
        label: `less memory. ${mb(rustrak.ram.peak)} for Rustrak at its busiest, ${mb(glitchtip.ram.peak)} for GlitchTip's stack.`,
      },
      {
        value: times(glitchtip.cpuMsPerEvent, rustrak.cpuMsPerEvent),
        label: `less CPU per event: ${rustrak.cpuMsPerEvent} ms against ${glitchtip.cpuMsPerEvent} ms, databases included.`,
      },
      {
        value: `${rustrak.storedSecs} s`,
        label: `for Rustrak to store all ${num(SETUP.events)}. GlitchTip needed ${glitchtip.storedSecs} s.`,
      },
    ],
    stories: [
      {
        visual: 'replay',
        title: 'The burst, replayed',
        lead: `GlitchTip stored as it received, about ${num(glitchtip.storedPerS)} events a second, and had the last one at ${glitchtip.storedSecs} seconds. Rustrak was done at ${rustrak.storedSecs}.`,
        caption:
          'Played back at twice the real speed from our measurements: counts every 2 seconds, a straight line in between.',
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
        a: `Rustrak. Under the same 20,000-event burst it peaked at ${mb(rustrak.ram.peak)}; GlitchTip with its PostgreSQL and Valkey peaked at ${mb(glitchtip.ram.peak)}.`,
      },
      {
        q: 'Do I need PostgreSQL for Rustrak?',
        a: 'No. Rustrak stores everything in one SQLite file by default. PostgreSQL is an option, with its own image.',
      },
      {
        q: 'How do I move from GlitchTip to Rustrak?',
        a: 'Change the DSN in your apps. Both speak the Sentry protocol, so the SDK code stays as it is. Past events stay where they were.',
      },
    ],
  },
  {
    slug: 'bugsink',
    name: 'Bugsink',
    title: 'Rustrak vs Bugsink: memory, speed and setup compared',
    description: `Bugsink vs Rustrak, both one container on SQLite, same 4 cores and 20,000 errors: ${bugsinkLite.storedSecs} s vs ${rustrak.storedSecs} s to store them, ${bugsinkLite.cpuMsPerEvent} vs ${rustrak.cpuMsPerEvent} ms of CPU per event.`,
    headline: 'Rustrak vs Bugsink',
    lead: 'Both install as one container on SQLite and take events from the Sentry SDKs. We ran both that way on the same four cores, then ran Bugsink again on PostgreSQL from its own Compose file.',
    runs: ['rustrak', 'rustrak-pg', 'bugsink-sqlite', 'bugsink'],
    stats: [
      {
        value: times(bugsinkLite.storedSecs, rustrak.storedSecs),
        label: `faster to store ${num(SETUP.events)} events: ${rustrak.storedSecs} s against ${bugsinkLite.storedSecs} s, both on SQLite.`,
      },
      {
        value: times(bugsinkLite.cpuMsPerEvent, rustrak.cpuMsPerEvent),
        label: `less CPU per event: ${rustrak.cpuMsPerEvent} ms against ${bugsinkLite.cpuMsPerEvent} ms.`,
      },
      {
        value: times(bugsinkLite.ram.peak, rustrak.ram.peak),
        label: `less memory: ${mb(rustrak.ram.peak)} against ${mb(bugsinkLite.ram.peak)}, one container each.`,
      },
    ],
    stories: [
      {
        visual: 'core',
        title: 'What one CPU core buys',
        lead: `Divide a second of CPU by what each event cost. Rustrak gets through ${num(Math.round(1000 / rustrak.cpuMsPerEvent))} events; Bugsink on the same SQLite, ${num(Math.round(1000 / bugsinkLite.cpuMsPerEvent))}.`,
        caption:
          'From the CPU per event we measured, the whole stack counted. One dot is ten events.',
      },
      {
        visual: 'replay',
        title: 'The burst, replayed',
        lead: `On SQLite Bugsink had the last event at ${bugsinkLite.storedSecs} seconds; on PostgreSQL at ${bugsink.storedSecs}. Rustrak was done at ${rustrak.storedSecs}.`,
        caption:
          'Played back at ten times the real speed from our measurements: counts every 2 seconds, a straight line in between.',
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
        a: `Both on SQLite, Rustrak stored a burst of 20,000 events in ${rustrak.storedSecs} seconds and Bugsink in ${bugsinkLite.storedSecs}, using ${rustrak.cpuMsPerEvent} ms and ${bugsinkLite.cpuMsPerEvent} ms of CPU per event.`,
      },
      {
        q: 'Does Rustrak do more than errors?',
        a: 'Yes: tracing, logs, cron monitors, releases with crash-free sessions and user feedback, all from the same Sentry SDK you already send errors with.',
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
