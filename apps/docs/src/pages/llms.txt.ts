import type { APIRoute } from 'astro';
import { getPosts } from '../blog';
import { getDocs, slugOf } from '../docs';
import { TABS } from '../docs-nav';
import { num, SETUP } from '../lib/compare';
import { COMPETITORS } from '../lib/vs';
import { SITE } from '../llms';
import { DEFINITION } from '../seo';

/**
 * `/llms.txt`: every page in reading order, one line each with what it
 * covers, for an assistant deciding what to read. The pages themselves are
 * their `.md` URLs.
 */
export const GET: APIRoute = async () => {
  const docs = new Map((await getDocs()).map((page) => [slugOf(page), page]));
  const lines = [
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

  for (const tab of TABS) {
    for (const group of tab.groups) {
      lines.push(`## ${tab.title}: ${group.title}`, '');
      for (const item of group.items) {
        const page = docs.get(item.slug);
        if (!page) continue;
        lines.push(
          `- [${item.title}](${SITE}/${item.slug}.md): ${page.data.description}`,
        );
      }
      lines.push('');
    }
  }

  lines.push('## Blog', '');
  for (const post of await getPosts()) {
    lines.push(
      `- [${post.data.title}](${SITE}/blog/${post.id}.md): ${post.data.description}`,
    );
  }

  lines.push(
    '',
    '## Benchmarks and comparisons',
    '',
    `- [Benchmark](${SITE}/benchmarks.md): Rustrak, self-hosted Sentry, GlitchTip and Bugsink on the same machine and the same burst of ${num(SETUP.events)} errors: time to store them, memory, CPU per event and disk`,
    ...COMPETITORS.map(
      (c) => `- [${c.headline}](${SITE}/vs/${c.slug}): ${c.description}`,
    ),
    '',
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

  return new Response(lines.join('\n'), {
    headers: { 'Content-Type': 'text/plain; charset=utf-8' },
  });
};
