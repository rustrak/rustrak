import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { contrastRatio, parseColor } from './color';

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

const primitives = declarationsIn(':root');
const dark = declarationsIn(':root,\\n\\[data-theme="dark"\\]');
const light = declarationsIn('\\[data-theme="light"\\]');

/**
 * Follows a `var(--x)` chain down to a literal.
 *
 * The theme is asked first and the primitives second, which is exactly the
 * cascade a browser would apply: a semantic name can point at another semantic
 * name, and only the bottom of the chain is a raw value.
 */
function resolve(name: string, theme: Map<string, string>): string {
  const seen = new Set<string>();
  let value = theme.get(name) ?? primitives.get(name);

  while (value?.startsWith('var(')) {
    const next = value.slice(4, -1).trim();
    if (seen.has(next)) throw new Error(`Cycle resolving ${name}`);
    seen.add(next);
    value = theme.get(next) ?? primitives.get(next);
  }

  if (value == null) throw new Error(`Undeclared token: ${name}`);
  return value;
}

function ratio(fg: string, bg: string, theme: Map<string, string>): number {
  return contrastRatio(
    parseColor(resolve(fg, theme)),
    parseColor(resolve(bg, theme)),
  );
}

/** Two decimals, so the assertion reads like the number a designer quotes. */
const round = (n: number) => Math.round(n * 100) / 100;

/*
 * The ratios are pinned rather than bounded.
 *
 * A `toBeGreaterThan` would let a palette drift downwards for years without
 * anyone noticing, as long as it stayed over the line. Pinning the number means
 * any change to a colour shows up in the diff as a number moving, and whoever
 * moved it has to say so.
 */
describe('dark theme', () => {
  const on = (fg: string, bg: string) => round(ratio(fg, bg, dark));

  it('reads text on the canvas', () => {
    expect(on('--fg', '--surface-canvas')).toBe(13.02);
    expect(on('--fg-secondary', '--surface-canvas')).toBe(10.37);
    expect(on('--fg-tertiary', '--surface-canvas')).toBe(7.57);
    expect(on('--fg-muted', '--surface-canvas')).toBe(6.28);
    expect(on('--fg-subtle', '--surface-canvas')).toBe(5.38);
  });

  it('reads text on a panel and on a surface', () => {
    expect(on('--fg', '--surface-panel')).toBe(11.5);
    expect(on('--fg', '--surface')).toBe(10.9);
    expect(on('--fg-muted', '--surface')).toBe(5.26);
    expect(on('--fg-subtle', '--surface')).toBe(4.51);
  });

  it('reads lime everywhere it lands', () => {
    expect(on('--fg-brand', '--surface-canvas')).toBe(14.43);
    expect(on('--fg-brand', '--surface')).toBe(12.08);
    // The primary button: near-black on lime, never white.
    expect(on('--fg-on-brand', '--surface-brand')).toBe(14.38);
  });

  /*
   * The tooltip is the one surface in the system that is not the same *kind* of
   * surface in both themes: a raised chip in dark, an inverted one in light. It
   * therefore carries its own foreground, and this is the pair that proves it.
   * Written against `--fg` instead, the light tooltip was 1:1 -- black text on
   * a black chip, invisible, and nothing else in the suite noticed.
   */
  it('reads the tooltip on its own chip', () => {
    expect(on('--fg-on-tooltip', '--surface-tooltip')).toBe(11.71);
  });

  it('reads severity text on the surface it is written on', () => {
    expect(on('--sev-error-fg', '--surface-panel')).toBe(5.73);
    expect(on('--sev-error-fg', '--surface')).toBe(5.43);
    expect(on('--sev-warning-fg', '--surface-panel')).toBe(7.03);
    expect(on('--sev-warning-fg', '--surface')).toBe(6.66);
  });
});

/*
 * The supporting greys, and why three of them do not clear 4.5:1.
 *
 * Below `--fg-subtle` the scale is not body text. It is what a row says about
 * itself once you are already reading the row: a timestamp beside an id, a
 * separator, a placeholder, a chevron. None of them ever carries meaning on its
 * own -- every one repeats or annotates something set in a tone that does clear
 * AA. They are pinned here so a palette change shows up as a number in the diff
 * rather than as nothing at all.
 */
describe('dark theme · the supporting greys are below AA on purpose', () => {
  const on = (fg: string) => round(ratio(fg, '--surface-panel', dark));

  it('pins what each one costs', () => {
    expect(on('--fg-meta')).toBe(3.58);
    expect(on('--fg-ghost')).toBe(2.45);
    expect(on('--fg-placeholder')).toBe(2.45);
  });

  it('keeps everything above them at AA', () => {
    for (const token of [
      '--fg',
      '--fg-secondary',
      '--fg-tertiary',
      '--fg-muted',
      '--fg-subtle',
      '--fg-brand',
      '--sev-error-fg',
      '--sev-warning-fg',
      '--sev-info-fg',
    ]) {
      expect(
        round(ratio(token, '--surface-panel', dark)),
      ).toBeGreaterThanOrEqual(4.5);
    }
  });
});

describe('light theme', () => {
  const on = (fg: string, bg: string) => round(ratio(fg, bg, light));

  it('reads text on the canvas', () => {
    expect(on('--fg', '--surface-canvas')).toBe(15.91);
    expect(on('--fg-secondary', '--surface-canvas')).toBe(12.32);
    expect(on('--fg-tertiary', '--surface-canvas')).toBe(8.95);
    expect(on('--fg-muted', '--surface-canvas')).toBe(6.05);
  });

  /*
   * The light theme's button is a bright lime carrying near-black ink, as in
   * dark: white on a lime that bright would be about 1.5:1. Lime as *text*
   * (`--fg-brand`) stays the deep olive, the only lime that reads on white.
   */
  it('reads ink on the light theme lime, and lime text on the canvas', () => {
    expect(on('--fg-on-brand', '--surface-brand')).toBe(12.32);
    expect(on('--fg-brand', '--surface-canvas')).toBe(5.36);
  });

  it('reads the tooltip on its own chip', () => {
    expect(on('--fg-on-tooltip', '--surface-tooltip')).toBe(17.04);
    // What it would be if the tooltip used `--fg` like every other surface.
    expect(on('--fg', '--surface-tooltip')).toBe(1);
  });

  it('reads severity text on a card', () => {
    expect(on('--sev-error-fg', '--surface')).toBe(5.98);
    expect(on('--sev-warning-fg', '--surface')).toBe(5.32);
  });

  it('keeps the readable tones at AA', () => {
    for (const token of [
      '--fg',
      '--fg-secondary',
      '--fg-tertiary',
      '--fg-muted',
      '--fg-subtle',
      '--fg-brand',
      '--sev-error-fg',
      '--sev-warning-fg',
      '--sev-info-fg',
    ]) {
      expect(round(ratio(token, '--surface', light))).toBeGreaterThanOrEqual(
        4.5,
      );
    }
  });
});
