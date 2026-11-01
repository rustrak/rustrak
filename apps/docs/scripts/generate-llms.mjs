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

const SITE = 'https://rustrak.dev';
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
  const pages = [];
  for (const slug of Object.keys(await meta(dir))) {
    const file = join(CONTENT, dir, `${slug}.mdx`);
    const folderIndex = join(CONTENT, dir, slug, 'index.mdx');
    if (slug === 'index') pages.push([`/${dir}`, file]);
    else if (await exists(file)) pages.push([`/${dir}/${slug}`, file]);
    else if (await exists(folderIndex))
      pages.push(...(await pagesOf(`${dir}/${slug}`)));
  }
  return pages;
}

const exists = (path) =>
  readFile(path).then(
    () => true,
    () => false,
  );

// MDX to Markdown: drop the frontmatter, imports and JSX-only lines (demos),
// and make site links absolute so they work outside the site.
function toMarkdown(source) {
  const description = source
    .match(/^---\n(?:[\s\S]*?\n)?description:\s*(.+)\n[\s\S]*?---\n/)?.[1]
    ?.replace(/^"(.*)"$/, '$1');
  const body = source
    .replace(/^---\n[\s\S]*?\n---\n/, '')
    .replace(/^import .*\n/gm, '')
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
  '> Self-hosted error tracking compatible with the Sentry SDKs. One Rust server, SQLite or PostgreSQL, that also takes transactions, logs, release health, cron check-ins, user feedback and AI agent traces.',
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

index.push(
  '## API',
  '',
  `- [OpenAPI spec](${SITE}/openapi.json): Every endpoint, with its parameters, schemas and authentication`,
  `- [API reference](${SITE}/api-reference): The same spec as an interactive page`,
  '',
  '## Optional',
  '',
  `- [Changelog](${SITE}/changelog): What changed in each release`,
  `- [All docs in one file](${SITE}/llms-full.txt): Every page above, concatenated`,
  '',
);

await writeFile(join(PUBLIC, 'llms.txt'), index.join('\n'));
await writeFile(
  join(PUBLIC, 'llms-full.txt'),
  `# Rustrak documentation\n\n${full.join('\n\n---\n\n')}\n`,
);
console.log(`llms.txt: ${full.length} pages`);
