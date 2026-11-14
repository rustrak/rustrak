import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

/**
 * The highlighter registers only the grammars the dashboard uses: the full
 * Prism build is ~580 KB of grammars for some 300 languages. The cost of that
 * is that a snippet in a language nobody registered renders as plain text
 * without a word. So this reads every place that names a language and fails
 * until `syntax-highlighter.ts` registers it.
 *
 * Read as text, like the other rules here: importing the grammars into Vitest
 * would only prove they load, not that the list is complete.
 */

const src = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const read = (path: string) => readFileSync(resolve(src, path), 'utf8');

function matches(path: string, pattern: RegExp): string[] {
  return [...read(path).matchAll(pattern)].map((match) => match[1] as string);
}

/** The values of `LANGUAGE_BY_EXTENSION` in the stack frame. */
function stackFrameLanguages(): string[] {
  const file = read('features/event/ui/components/stack-frame-item.tsx');
  const start = file.indexOf('LANGUAGE_BY_EXTENSION');
  const table = file.slice(start, file.indexOf('};', start));
  return [...table.matchAll(/\w+: '([\w-]+)'/g)].map((m) => m[1] as string);
}

/** The keys of `GRAMMARS` in `syntax-highlighter.ts`. */
function registered(): string[] {
  const file = read('shared/ui/components/syntax-highlighter.ts');
  const start = file.indexOf('GRAMMARS');
  const table = file.slice(start, file.indexOf('};', start));
  return [...table.matchAll(/^\s+'?([\w-]+)'?[,:]/gm)].map(
    (m) => m[1] as string,
  );
}

describe('syntax highlighter languages', () => {
  const used = new Set([
    ...matches(
      'features/project/lib/platform-snippets.ts',
      /language: '([\w-]+)'/g,
    ),
    ...matches('features/mcp/lib/snippets.ts', /language: '([\w-]+)'/g),
    ...matches('features/monitor/lib/setup.ts', /\['([\w-]+)',/g),
    ...stackFrameLanguages(),
  ]);
  // `text` is the stack frame's "no idea": plain text on purpose.
  used.delete('text');

  it('finds the languages it checks', () => {
    expect(used.size).toBeGreaterThan(10);
  });

  it.each([...used])('registers %s', (language) => {
    expect(registered()).toContain(language);
  });
});
