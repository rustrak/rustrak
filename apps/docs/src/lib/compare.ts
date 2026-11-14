/**
 * The comparison benchmark behind `/benchmarks` and the `/vs/*` pages.
 *
 * Every figure is copied from `packages/benchmarks/results/compare/round1/*.json`,
 * written by `packages/benchmarks/compare/run.sh`. Re-run it and copy the new
 * numbers here; never round in a product's favour or type one in by hand.
 */

export const MEASURED = '2026-11-14';
export const HARNESS =
  'https://github.com/rustrak/rustrak/tree/main/packages/benchmarks/compare';

export const SETUP = {
  host: 'Docker Desktop on an Apple silicon Mac (linux/arm64 VM, 12 CPUs, 16 GB RAM, 1 GB swap)',
  cores: 4,
  events: 20_000,
  concurrency: 16,
  eventKb: 20,
  groups: 2_000,
};

export type Run = {
  id: string;
  /** Product name, as the tables print it. */
  name: string;
  /** Which deployment of it: the database or Sentry's compose profile. */
  variant: string;
  version: string;
  rustrak?: boolean;
  containers: number;
  images: string[];
  imageMb: number;
  /** From `docker compose up` to a healthy health check, install excluded. */
  readySecs: number;
  /** cgroup `anon` summed over the stack's containers, in MB. */
  ram: { idle: number; peak: number; after: number };
  /** Events the endpoint answered with a 2xx, per second, over the burst. */
  acceptPerS: number;
  p99Ms: number;
  /** Events queryable in the database, per second, from the first request. */
  storedPerS: number;
  storedSecs: number;
  cpuMsPerEvent: number;
  diskMb: number;
};

export const RUNS: Run[] = [
  {
    id: 'rustrak',
    name: 'Rustrak',
    variant: 'SQLite',
    version: '0.27.0',
    rustrak: true,
    containers: 1,
    images: ['rustrak/rustrak-server:v0.27.0'],
    imageMb: 100,
    readySecs: 0,
    ram: { idle: 51.1, peak: 88.4, after: 86.0 },
    acceptPerS: 3697,
    p99Ms: 7.5,
    storedPerS: 3333,
    storedSecs: 6,
    cpuMsPerEvent: 0.58,
    diskMb: 304.7,
  },
  {
    id: 'rustrak-pg',
    name: 'Rustrak',
    variant: 'PostgreSQL',
    version: '0.27.0',
    rustrak: true,
    containers: 2,
    images: ['rustrak/rustrak-server:v0.27.0-postgres', 'postgres:16-alpine'],
    imageMb: 510,
    readySecs: 9,
    ram: { idle: 62.2, peak: 162.3, after: 112.2 },
    acceptPerS: 4044,
    p99Ms: 13.3,
    storedPerS: 2222,
    storedSecs: 9,
    cpuMsPerEvent: 1.07,
    diskMb: 209.1,
  },
  {
    id: 'glitchtip',
    name: 'GlitchTip',
    variant: 'all-in-one',
    version: '7.0.0',
    containers: 3,
    images: ['glitchtip/glitchtip:7.0.0', 'postgres:18', 'valkey/valkey:9'],
    imageMb: 1853,
    readySecs: 6,
    ram: { idle: 205.0, peak: 422.7, after: 405.3 },
    acceptPerS: 712,
    p99Ms: 87.3,
    storedPerS: 690,
    storedSecs: 29,
    cpuMsPerEvent: 2.3,
    diskMb: 297.2,
  },
  {
    id: 'bugsink-sqlite',
    name: 'Bugsink',
    variant: 'SQLite',
    version: '2.6.3',
    containers: 1,
    images: ['bugsink/bugsink:2.6.3'],
    imageMb: 418,
    readySecs: 5,
    ram: { idle: 308.5, peak: 358.1, after: 360.9 },
    acceptPerS: 1440,
    p99Ms: 17.9,
    storedPerS: 127,
    storedSecs: 157,
    cpuMsPerEvent: 9.64,
    diskMb: 420.6,
  },
  {
    id: 'bugsink',
    name: 'Bugsink',
    variant: 'PostgreSQL',
    version: '2.6.3',
    containers: 2,
    images: ['bugsink/bugsink:2.6.3', 'postgres:17-alpine'],
    imageMb: 834,
    readySecs: 11,
    ram: { idle: 341.3, peak: 378.1, after: 361.0 },
    acceptPerS: 407,
    p99Ms: 49.5,
    storedPerS: 71,
    storedSecs: 280,
    cpuMsPerEvent: 26.47,
    diskMb: 758.5,
  },
  {
    id: 'sentry-errors',
    name: 'Sentry',
    variant: 'errors-only',
    version: '26.10.1',
    containers: 28,
    images: [],
    imageMb: 10036,
    readySecs: 25,
    ram: { idle: 7381.8, peak: 8918.8, after: 8646.3 },
    acceptPerS: 13697,
    p99Ms: 2.1,
    storedPerS: 189,
    storedSecs: 106,
    cpuMsPerEvent: 18.53,
    diskMb: 2366.7,
  },
  {
    id: 'sentry',
    name: 'Sentry',
    variant: 'feature-complete',
    version: '26.10.1',
    containers: 53,
    images: [],
    imageMb: 12411,
    readySecs: 35,
    ram: { idle: 12954.0, peak: 14357.8, after: 14125.9 },
    acceptPerS: 12704,
    p99Ms: 2.6,
    storedPerS: 163,
    storedSecs: 123,
    cpuMsPerEvent: 22.49,
    diskMb: 2369.6,
  },
];

export const run = (id: string) => {
  const found = RUNS.find((r) => r.id === id);
  if (!found) throw new Error(`no benchmark run "${id}"`);
  return found;
};

export const label = (r: Run) => `${r.name} (${r.variant})`;

/** 88.4 → "88 MB", 8918.8 → "8.9 GB". */
export function mb(value: number) {
  return value >= 1000
    ? `${(value / 1000).toLocaleString('en-US', { maximumFractionDigits: 1 })} GB`
    : `${Math.round(value).toLocaleString('en-US')} MB`;
}

export const num = (value: number) => value.toLocaleString('en-US');

/** How many times `a` is `b`, rounded to what the precision supports. */
export function times(a: number, b: number) {
  const r = a / b;
  return `${r >= 10 ? Math.round(r).toLocaleString('en-US') : r.toFixed(1)}×`;
}
