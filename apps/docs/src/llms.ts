import type { Post } from './blog';
import type { DocPage } from './docs';
import { HARNESS, label, MEASURED, mb, num, RUNS, SETUP } from './lib/compare';

/*
 * The site in the shapes agents read (https://llmstxt.org): each page as
 * Markdown, an index of them (`/llms.txt`) and all of them in one file
 * (`/llms-full.txt`). Built from the same content as the pages.
 */

export const SITE = 'https://rustrak.dev';

/** Site links made absolute, so the Markdown works away from the site. */
const absolute = (markdown: string) =>
  markdown.replace(/\]\(\/(?!\/)/g, `](${SITE}/`);

/** A docs page as Markdown: its body, which opens with its `# Title`, less
 * the interactive demos, which mean nothing as text. */
export const docMarkdown = (page: DocPage) =>
  absolute(
    (page.body ?? '')
      .replace(/^\s*<InteractiveDemo[^>]*\/>\s*$/gm, '')
      .replace(/\n{3,}/g, '\n\n')
      .trim(),
  );

export const postMarkdown = (post: Post) =>
  absolute(`# ${post.data.title}\n\n${post.body?.trim() ?? ''}`);

/** The benchmark as a Markdown table, since its page draws it in charts. */
export const benchmarkMarkdown = () =>
  [
    '# Benchmark: Rustrak vs Sentry, GlitchTip and Bugsink',
    '',
    `Measured ${MEASURED}. ${SETUP.host}. Each product ran alone from its own Docker Compose file, pinned to the same ${SETUP.cores} cores, and received the same burst of ${num(SETUP.events)} gzipped Sentry envelopes (about ${SETUP.eventKb} KB each, ${SETUP.concurrency} in flight, ${num(SETUP.groups)} distinct issues). Memory is the cgroup \`anon\` counter summed over the stack; CPU is the stack's CPU time from the first request to the last event stored, divided by the events. Every product stored every event. Script: ${HARNESS}`,
    '',
    '| Product | Containers | Memory at rest | Memory at peak | Accepted/s | All stored after | CPU per event | Disk after |',
    '|---|---|---|---|---|---|---|---|',
    ...RUNS.map(
      (r) =>
        `| ${label(r)} ${r.version} | ${r.containers} | ${mb(r.ram.idle)} | ${mb(r.ram.peak)} | ${num(r.acceptPerS)} | ${r.storedSecs} s | ${r.cpuMsPerEvent} ms | ${mb(r.diskMb)} |`,
    ),
    '',
    'Sentry accepts fastest because Relay answers once an event is on Kafka; storing it in ClickHouse is the slow part. One run, in a Docker Desktop VM; only ingestion was measured.',
  ].join('\n');
