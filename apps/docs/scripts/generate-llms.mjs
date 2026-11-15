// Writes the docs in the shapes agents read (https://llmstxt.org, v2):
//
//   public/llms.txt        an index of every page, one line each, in reading order
//   public/llms-full.txt   every page in one file
//   public/<page>.md       each page as plain Markdown, next to its HTML
//
// All three are generated from content/ at build time and git-ignored.
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { getPost, getPosts } from '../src/lib/blog.ts';
import {
  HARNESS,
  label,
  MEASURED,
  mb,
  num,
  RUNS,
  SETUP,
} from '../src/lib/compare.ts';
import { DEFINITION, SITE } from '../src/lib/seo.ts';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const CONTENT = join(ROOT, 'content');
const PUBLIC = join(ROOT, 'public');

// Sections in reading order: what to do first, then reference. Changelog and blog are written at release
// time and only linked, under Optional.
const SECTIONS = [
  ['getting-started', 'Getting Started'],
  ['usage', 'Usage'],
  ['configuration', 'Configuration'],
  ['sdks', 'SDKs & Integrations'],
  ['troubleshooting', 'Troubleshooting'],
  ['upgrading', 'Upgrading'],
  ['reference', 'Reference'],
];

// Pages whose first paragraph does not describe them, and that cannot carry
// frontmatter because they are generated.
const DESCRIPTIONS = {
  '/sdks/client':
    'The TypeScript client for the Rustrak API: installation, authentication, errors and every method.',
};

const meta = async (dir) =>
  (await import(pathToFileURL(join(CONTENT, dir, '_meta.js')).href)).default;

// The pages of a folder as [urlPath, file], following its _meta.js. A sub-folder
// (usage/alerts) takes its index as the folder's own URL.
async function pagesOf(dir) {
  const slugs = Object.keys(await meta(dir));
  const pages = await Promise.all(
    slugs.map(async (slug) => {
      const file = join(CONTENT, dir, `${slug}.mdx`);
      const folderIndex = join(CONTENT, dir, slug, 'index.mdx');
      if (slug === 'index') return [[`/${dir}`, file]];
      if (await exists(file)) return [[`/${dir}/${slug}`, file]];
      if (await exists(folderIndex)) return pagesOf(`${dir}/${slug}`);
      return [];
    }),
  );
  return pages.flat();
}

const exists = (path) =>
  readFile(path).then(
    () => true,
    () => false,
  );

// MDX to Markdown: drop the frontmatter, imports and JSX-only lines (demos),
// and make site links absolute so they work outside the site. Imports inside
// code fences are the examples' own and stay.
function toMarkdown(source) {
  const description = source
    .match(/^---\n(?:[\s\S]*?\n)?description:\s*(.+)\n[\s\S]*?---\n/)?.[1]
    ?.replace(/^"(.*)"$/, '$1');
  let fenced = false;
  const body = source
    .replace(/^---\n[\s\S]*?\n---\n/, '')
    .split('\n')
    .filter((line) => {
      if (/^\s*```/.test(line)) fenced = !fenced;
      return fenced || !line.startsWith('import ');
    })
    .join('\n')
    .replace(/^<[A-Z][^\n]*\/>\n\n?/gm, '')
    .replace(/\]\(\/(?!\/)/g, `](${SITE}/`)
    .trim();
  const title = body.match(/^# (.+)$/m)?.[1] ?? '';
  return { title, description: description ?? firstSentences(body), body };
}

// The first paragraph after the title, as plain text, cut to two sentences.
function firstSentences(body) {
  const paragraph = body
    .replace(/^# .+\n+/, '')
    .split(/\n\n/)
    .find((p) => p && !/^[#|`<>-]/.test(p.trim()));
  if (!paragraph) return '';
  const text = paragraph
    .replace(/\[([^\]]+)\]\([^)]+\)/g, '$1')
    .replace(/[*`]/g, '')
    .replace(/\s+/g, ' ')
    .trim();
  return text.match(/^(.*?[.!?])(\s+.*?[.!?])?(\s|$)/)?.[0].trim() ?? text;
}

const index = [
  '# Rustrak',
  '',
  `> ${DEFINITION} Besides errors it takes transactions, logs, release health, cron check-ins, user feedback and AI agent traces.`,
  '',
  'Notes for agents:',
  '',
  '- Apps send to Rustrak with the official Sentry SDKs. Only the DSN changes; never suggest a Rustrak-specific SDK.',
  "- The DSN host comes from the server's `PUBLIC_URL`. A DSN with `0.0.0.0` means it is unset.",
  `- The HTTP API is described by the OpenAPI spec at ${SITE}/openapi.json. A running server also serves an MCP endpoint at \`/mcp\` once an admin turns it on.`,
  '- Rustrak runs as one process per database. Do not suggest several instances behind a load balancer.',
  '',
];
const full = [];

for (const [dir, name] of SECTIONS) {
  index.push(`## ${name}`, '');
  for (const [url, file] of await pagesOf(dir)) {
    const page = toMarkdown(await readFile(file, 'utf8'));
    const out = join(PUBLIC, `${url}.md`);
    await mkdir(dirname(out), { recursive: true });
    await writeFile(out, `---\nurl: ${SITE}${url}\n---\n\n${page.body}\n`);
    const description = DESCRIPTIONS[url] ?? page.description;
    index.push(
      `- [${page.title}](${SITE}${url}.md)${description ? `: ${description}` : ''}`,
    );
    full.push(`<!-- ${SITE}${url} -->\n\n${page.body}`);
  }
  index.push('');
}

// Blog posts: the long write-ups (benchmarks, how a feature works) that a
// docs page links to but does not repeat. Read through the same `getPosts()`
// the blog pages use, so drafts, order and frontmatter parsing always agree
// with what the site publishes.
index.push('## Blog', '');
await mkdir(join(PUBLIC, 'blog'), { recursive: true });
for (const post of getPosts()) {
  const url = `/blog/${post.slug}`;
  const { body } = toMarkdown(getPost(post.slug).content);
  const page = `# ${post.title}\n\n${body.replace(/^# .+\n+/, '')}`;
  await writeFile(
    join(PUBLIC, `${url}.md`),
    `---\nurl: ${SITE}${url}\n---\n\n${page}\n`,
  );
  index.push(`- [${post.title}](${SITE}${url}.md): ${post.description}`);
  full.push(`<!-- ${SITE}${url} -->\n\n${page}`);
}
index.push('');

// The comparison benchmark: its numbers as a Markdown table at
// /benchmarks.md, since the page itself draws them in charts.
const benchmark = [
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
await writeFile(
  join(PUBLIC, 'benchmarks.md'),
  `---\nurl: ${SITE}/benchmarks\n---\n\n${benchmark}\n`,
);
full.push(`<!-- ${SITE}/benchmarks -->\n\n${benchmark}`);

index.push(
  '## Benchmarks and comparisons',
  '',
  `- [Benchmark](${SITE}/benchmarks.md): Rustrak, self-hosted Sentry, GlitchTip and Bugsink on the same machine and the same burst of ${num(SETUP.events)} errors: time to store them, memory, CPU per event and disk`,
  `- [Rustrak vs self-hosted Sentry](${SITE}/vs/sentry): Memory, containers and time to store the burst, and what switching takes`,
  `- [Rustrak vs GlitchTip](${SITE}/vs/glitchtip): Memory, CPU per event and time to store the burst`,
  `- [Rustrak vs Bugsink](${SITE}/vs/bugsink): Both on SQLite: time to store the burst, CPU per event and memory`,
  '',
);

index.push(
  '## API',
  '',
  `- [OpenAPI spec](${SITE}/openapi.json): Every endpoint, with its parameters, schemas and authentication`,
  `- [API reference](${SITE}/api-reference): The same spec as an interactive page`,
  '',
  '## Optional',
  '',
  `- [Changelog](${SITE}/changelog): What changed in each release`,
  `- [All docs in one file](${SITE}/llms-full.txt): Every page and post above, concatenated`,
  '',
);

await writeFile(join(PUBLIC, 'llms.txt'), index.join('\n'));
await writeFile(
  join(PUBLIC, 'llms-full.txt'),
  `# Rustrak documentation\n\n${full.join('\n\n---\n\n')}\n`,
);
console.log(`llms.txt: ${full.length} pages`);
