import type { APIRoute } from 'astro';
import { benchmarkMarkdown, SITE } from '../llms';

/** `/benchmarks.md`: the benchmark's numbers as a table, for assistants. */
export const GET: APIRoute = () =>
  new Response(
    `---\nurl: ${SITE}/benchmarks\n---\n\n${benchmarkMarkdown()}\n`,
    {
      headers: { 'Content-Type': 'text/markdown; charset=utf-8' },
    },
  );
