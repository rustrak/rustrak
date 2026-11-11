import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

/**
 * `tokens.css` read as data, for what runs in Node: the contrast table and
 * Storybook's manager theme. Never imported by a component.
 */
const css = readFileSync(
  fileURLToPath(new URL('./tokens.css', import.meta.url)),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '');

/** Every `--name: value` inside one selector's block. */
function declarationsIn(selector: string): Map<string, string> {
  const block = css.match(
    new RegExp(`${selector}\\s*\\{([\\s\\S]*?)\\n\\}`),
  )?.[1];
  const out = new Map<string, string>();

  for (const [, name, value] of (block ?? '').matchAll(
    /^\s{2}(--[a-z0-9-]+)\s*:\s*([^;]+);/gm,
  )) {
    if (name && value) out.set(name, value.trim());
  }

  return out;
}

export type Theme = Map<string, string>;

const primitives = declarationsIn(':root');
export const dark: Theme = declarationsIn(':root,\\n\\[data-theme="dark"\\]');
export const light: Theme = declarationsIn('\\[data-theme="light"\\]');
const scale = declarationsIn('@theme');

/**
 * Follows a `var(--x)` chain down to a literal.
 *
 * The theme is asked first and the primitives second, which is exactly the
 * cascade a browser would apply: a semantic name can point at another semantic
 * name, and only the bottom of the chain is a raw value.
 */
export function resolve(name: string, theme: Theme): string {
  const seen = new Set<string>();
  const lookup = (key: string) =>
    theme.get(key) ?? primitives.get(key) ?? scale.get(key);
  let value = lookup(name);

  while (value?.startsWith('var(')) {
    const next = value.slice(4, -1).trim();
    if (seen.has(next)) throw new Error(`Cycle resolving ${name}`);
    seen.add(next);
    value = lookup(next);
  }

  if (value == null) throw new Error(`Undeclared token: ${name}`);
  return value;
}
