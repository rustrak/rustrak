// What the CLI looks like in a terminal: the wordmark, its intro, and the
// small formatters the commands share. Colour goes through `styleText`, which
// already says nothing to a pipe, to CI logs or under NO_COLOR.

import { setTimeout as sleep } from 'node:timers/promises';
import { stripVTControlCharacters, styleText } from 'node:util';

/** The dashboard's lime, `--rk-lime-500` in @rustrak/ui, and its nearest of 256. */
export const LIME = '\x1b[38;2;193;247;104m';
const LIME_256 = '\x1b[38;5;191m';
const FG_RESET = '\x1b[39m';

/** Three rows of ascender above the x-height. */
const x = (...rows: string[]) => ['', '', '', ...rows];

/**
 * `rustrak`, lowercase and heavy like the mark (Outfit 700), one `#` per
 * pixel and two-pixel stems. Two pixel rows make one line of half blocks, so
 * ten rows are five lines.
 */
const GLYPHS: Record<string, string[]> = {
  r: x('## ##', '#####', '###  ', '##   ', '##   ', '##   ', '##   '),
  u: x('##  ##', '##  ##', '##  ##', '##  ##', '##  ##', '##  ##', ' #####'),
  s: x(' #####', '##    ', '####  ', ' #### ', '   ###', '    ##', '##### '),
  t: [
    '',
    '  ##  ',
    '  ##  ',
    '######',
    '  ##  ',
    '  ##  ',
    '  ##  ',
    '  ##  ',
    '  ##  ',
    '   ###',
  ],
  a: x(' #### ', '    ##', ' #####', '##  ##', '##  ##', '##  ##', ' #####'),
  k: [
    '##    ',
    '##    ',
    '##    ',
    '##  ##',
    '## ## ',
    '####  ',
    '###   ',
    '####  ',
    '## ## ',
    '##  ##',
  ],
};
const ROWS = 10;

/** The wordmark, as lines of `▀▄█` and spaces, all of one width. */
export function wordmark(word = 'rustrak'): string[] {
  const rows = Array.from({ length: ROWS }, (_, row) =>
    [...word]
      .map((ch) => {
        const glyph = GLYPHS[ch]!;
        const width = Math.max(...glyph.map((r) => r.length));
        return (glyph[row] ?? '').padEnd(width);
      })
      .join(' '),
  );
  const lines: string[] = [];
  for (let row = 0; row < rows.length; row += 2) {
    const top = rows[row]!;
    const bottom = rows[row + 1]!;
    lines.push(
      [...top]
        .map((px, i) => {
          const below = bottom[i] === '#';
          if (px === '#') return below ? '█' : '▀';
          return below ? '▄' : ' ';
        })
        .join(''),
    );
  }
  return lines;
}

type Rgb = [number, number, number];
/** `--rk-lime-500`, and the near-white the trace's head burns at. */
const LIME_RGB: Rgb = [193, 247, 104];
const HEAT_RGB: Rgb = [246, 255, 228];
/** Columns of afterglow behind the head while the mark is traced. */
const GLOW = 7;
/** The share of the animation spent tracing; the rest is one glint. */
const TRACE = 0.62;

/** Colour depth: truecolor, 256 or 16. */
export type Depth = 24 | 8 | 4;

const mix = (a: Rgb, b: Rgb, k: number): Rgb =>
  a.map((v, i) => Math.round(v + (b[i]! - v) * k)) as Rgb;
const easeInOut = (t: number) =>
  t < 0.5 ? 4 * t ** 3 : 1 - (-2 * t + 2) ** 3 / 2;

/** An RGB as the nearest colour the terminal has. */
function fg([r, g, b]: Rgb, depth: Depth, heat: number): string {
  if (depth === 24) return `\x1b[38;2;${r};${g};${b}m`;
  if (depth === 8) {
    const q = (v: number) => Math.round((v / 255) * 5);
    return `\x1b[38;5;${16 + 36 * q(r) + 6 * q(g) + q(b)}m`;
  }
  // 16 colours: the head bright white, the rest bright green.
  return heat > 0.5 ? '\x1b[97m' : '\x1b[92m';
}

/**
 * One frame of the intro, `t` from 0 to 1. First the mark is traced left to
 * right like a span growing in the build, its head near-white and cooling
 * to lime behind it; then one glint crosses it and it rests in lime.
 */
export function frame(lines: string[], t: number, depth: Depth = 24): string[] {
  const width = lines[0]!.length;
  const tracing = t < TRACE;
  const head = easeInOut(Math.min(t / TRACE, 1)) * (width + GLOW);
  const glint =
    easeInOut(Math.max(0, (t - TRACE) / (1 - TRACE))) * (width + 8) - 4;
  /** How hot column `x` burns, 0 to 1; `undefined` while not yet traced. */
  const heatAt = (x: number): number | undefined => {
    if (t >= 1) return 0;
    if (!tracing) return Math.max(0, 1 - Math.abs(x - glint) / 4) * 0.8;
    const behind = head - x;
    return behind <= 0 ? undefined : Math.max(0, 1 - behind / GLOW);
  };
  return lines.map((line) => {
    let out = '';
    let last = '';
    for (let x = 0; x < width; x++) {
      const heat = heatAt(x);
      if (heat === undefined) {
        out += ' ';
        continue;
      }
      const color = fg(mix(LIME_RGB, HEAT_RGB, heat), depth, heat);
      if (color !== last) {
        out += color;
        last = color;
      }
      out += line[x];
    }
    return last ? `${out}${FG_RESET}` : out;
  });
}

/** How many colours the terminal shows; `undefined` for none. */
function depth(): Depth | undefined {
  const out = process.stdout;
  if (!out.hasColors?.()) return undefined;
  return out.hasColors(2 ** 24) ? 24 : out.hasColors(256) ? 8 : 4;
}

/** The narrowest terminal a trace row fits on one line of. */
const MIN_COLUMNS = 60;

/**
 * Whether output can be redrawn in place: a terminal wide enough, not CI,
 * not a dumb terminal, not a screen reader (`ACCESSIBLE`, as clack reads it).
 */
export function redraws(
  env: NodeJS.ProcessEnv = process.env,
  out: { isTTY?: boolean; columns?: number } = process.stdout,
): boolean {
  return Boolean(
    out.isTTY &&
      !env.CI &&
      env.TERM !== 'dumb' &&
      !env.ACCESSIBLE &&
      (out.columns ?? 0) >= MIN_COLUMNS,
  );
}

/** How long the intro takes, whatever the terminal's frame rate. */
const DURATION = 1100;

/**
 * Draws the wordmark, animated where a terminal can show it and still
 * everywhere else. Timed by the clock, not by frames: a slow terminal drops
 * frames rather than slowing down. Each frame goes inside synchronized
 * output (mode 2026), so terminals that know it paint it whole, without
 * flicker; the others ignore the brackets.
 */
export async function logo(tagline: string): Promise<void> {
  const lines = wordmark();
  const pad = (l: string) => `  ${l}`;
  const footer = `  ${styleText('dim', tagline)}`;
  const colors = depth();
  if (!colors || !redraws()) {
    const still = colors ? frame(lines, 1, colors) : lines;
    console.log(['', ...still.map(pad), footer, ''].join('\n'));
    return;
  }
  const out = process.stdout;
  const show = () => out.write('\x1b[?25h');
  const interrupt = () => {
    show();
    process.exit(130);
  };
  process.once('exit', show);
  process.once('SIGINT', interrupt);
  out.write('\x1b[?25l\n');
  try {
    const start = performance.now();
    for (let drawn = false; ; drawn = true) {
      const t = Math.min(1, (performance.now() - start) / DURATION);
      const up = drawn ? `\x1b[${lines.length}A\r` : '';
      out.write(
        `\x1b[?2026h${up}${frame(lines, t, colors).map(pad).join('\n')}\n\x1b[?2026l`,
      );
      if (t >= 1) break;
      await sleep(16);
    }
    out.write(`${footer}\n\n`);
  } finally {
    show();
    process.off('exit', show);
    process.off('SIGINT', interrupt);
  }
}

/** The package manager that ran the command, from npm_config_user_agent. */
export function packageManager(userAgent: string | undefined) {
  const name = userAgent?.split('/')[0];
  return name === 'pnpm' || name === 'yarn' || name === 'bun' ? name : 'npm';
}

/** The command that runs a package script: `bun build` and `npm build` are built-ins. */
export function runScript(pm: string, script: string): string {
  return pm === 'npm' || pm === 'bun'
    ? `${pm} run ${script}`
    : `${pm} ${script}`;
}

/** A size as Vite prints it: `145.41 kB`. */
export function bytes(n: number): string {
  if (n < 1000) return `${n} B`;
  if (n < 1000 ** 2) return `${(n / 1000).toFixed(2)} kB`;
  return `${(n / 1000 ** 2).toFixed(2)} MB`;
}

/** The crate a `Compiling <name> v<version>` line of cargo's names. */
export function crate(line: string): string | undefined {
  return /^\s*Compiling (\S+) v/.exec(stripVTControlCharacters(line))?.[1];
}

const EIGHTHS = ['', '▏', '▎', '▍', '▌', '▋', '▊', '▉'];

/**
 * A span in a trace waterfall: a bar `width` cells wide in all, drawn from
 * `start` to `end` out of `total`, in eighths of a cell. A span too short
 * to see is still a sliver, so no step disappears.
 */
export function span(
  start: number,
  end: number,
  total: number,
  width: number,
): string {
  const units = width * 8;
  const from = Math.min(Math.round((start / total) * units), units - 1);
  const cell = Math.floor(from / 8);
  const length = Math.min(
    Math.max(1, Math.round((end / total) * units) - from),
    (width - cell) * 8,
  );
  const bar = '█'.repeat(Math.floor(length / 8)) + EIGHTHS[length % 8];
  return `${' '.repeat(cell)}${bar}`.padEnd(width);
}

/** One bar split by share, `█ ▓ ░` in turn; every part above zero shows. */
export function stack(parts: number[], width: number): string {
  const total = parts.reduce((a, b) => a + b, 0) || 1;
  const cells = parts.map((p) =>
    p > 0 ? Math.max(1, Math.floor((p / total) * width)) : 0,
  );
  const largest = () => cells.indexOf(Math.max(...cells));
  let sum = cells.reduce((a, b) => a + b, 0);
  for (; sum > width; sum--) cells[largest()]!--;
  for (; sum < width; sum++) cells[largest()]!++;
  return cells.map((n, i) => '█▓░'[i % 3]!.repeat(n)).join('');
}

/** `12ms` under a second, `7.9s` above. */
export function duration(ms: number): string {
  return ms < 1000 ? `${Math.round(ms)}ms` : `${(ms / 1000).toFixed(1)}s`;
}

/** The missing-language warnings, one line per kind instead of one per language. */
export function groupWarnings(warnings: string[]): string[] {
  const kinds = { readme: 'No readme', 'src/messages': 'No texts' } as const;
  const missing = new Map<string, string[]>();
  const out: string[] = [];
  for (const warning of warnings) {
    const m = /^(readme|src\/messages)\/([\w-]+)\.(?:md|json) is missing/.exec(
      warning,
    );
    if (!m) {
      out.push(warning);
      continue;
    }
    const kind = kinds[m[1] as keyof typeof kinds];
    if (!missing.has(kind)) {
      missing.set(kind, []);
      out.push(kind);
    }
    missing.get(kind)!.push(m[2]!);
  }
  return out.map((line) =>
    missing.has(line)
      ? `${line} in ${missing.get(line)!.join(', ')}: those readers see English`
      : line,
  );
}

/** `text` in the dashboard's lime, where colour shows. */
export function lime(text: string): string {
  const out = process.stdout;
  if (!out.hasColors?.()) return text;
  return `${out.hasColors(2 ** 24) ? LIME : LIME_256}${text}${FG_RESET}`;
}

interface Row {
  doing: string;
  done: string;
  start: number;
  end?: number;
  failed?: boolean;
  detail: string;
}

/** The verbs' column, right-aligned as cargo sets its own. */
const GUTTER = 12;

/**
 * Steps drawn as a trace, the way Rustrak draws a request: one span per
 * step on a shared timeline, the running one growing in lime. Where nothing
 * redraws (a pipe, CI), each step is one plain line once all have run.
 */
export class Trace {
  private rows: Row[] = [];
  private readonly t0 = performance.now();
  private drawn = 0;
  private timer: NodeJS.Timeout | undefined;
  private readonly live = redraws();
  private readonly show = () => process.stdout.write('\x1b[?25h');
  /** Ctrl-C skips `exit`: the cursor comes back here instead. */
  private readonly interrupt = () => {
    this.show();
    process.exit(130);
  };

  private get now(): number {
    return performance.now() - this.t0;
  }

  private get running(): Row | undefined {
    const last = this.rows.at(-1);
    return last && last.end === undefined ? last : undefined;
  }

  begin(doing: string, done: string): void {
    this.end();
    this.rows.push({ doing, done, start: this.now, detail: '' });
    if (this.live && !this.timer) {
      process.stdout.write('\x1b[?25l');
      process.once('exit', this.show);
      process.once('SIGINT', this.interrupt);
      this.timer = setInterval(() => this.draw(), 80);
    }
    this.draw();
  }

  /** What the running step is on: the crate cargo compiles, say. */
  detail(text: string): void {
    const row = this.running;
    if (row) row.detail = text;
  }

  end(detail?: string): void {
    const row = this.running;
    if (!row) return;
    row.end = this.now;
    if (detail !== undefined) row.detail = detail;
    this.draw();
  }

  fail(): void {
    const row = this.running;
    if (!row) return;
    row.failed = true;
    this.end();
  }

  /** Details known only once everything ran, in the steps' order. */
  details(texts: (string | undefined)[]): void {
    texts.forEach((text, i) => {
      if (text !== undefined && this.rows[i]) this.rows[i].detail = text;
    });
  }

  /**
   * Stops redrawing, leaving the last frame; the time it all took. Where
   * nothing redraws, the steps are printed now, details and all.
   */
  close(): number {
    if (!this.live) {
      for (const row of this.rows) console.log(this.line(row, 0));
      return this.now;
    }
    clearInterval(this.timer);
    this.timer = undefined;
    this.draw();
    this.show();
    process.off('exit', this.show);
    process.off('SIGINT', this.interrupt);
    return this.now;
  }

  private line(row: Row, width: number): string {
    const end = row.end ?? this.now;
    const verb = (
      row.failed ? 'Failed' : row.end === undefined ? row.doing : row.done
    ).padStart(GUTTER);
    const head = row.failed
      ? styleText(['bold', 'red'], verb)
      : row.end === undefined
        ? lime(styleText('bold', verb))
        : styleText('bold', verb);
    const time = styleText('dim', duration(end - row.start).padStart(6));
    if (width === 0) return `${head} ${time}  ${styleText('dim', row.detail)}`;
    const bar = span(row.start, end, Math.max(this.now, 1), width);
    const drawn = row.failed
      ? styleText('red', bar)
      : row.end === undefined
        ? lime(bar)
        : styleText('gray', bar);
    const room = (process.stdout.columns || 80) - GUTTER - width - 12;
    return `${head} ${drawn}  ${time}  ${styleText('dim', row.detail.slice(0, Math.max(room, 0)))}`;
  }

  private draw(): void {
    if (!this.live) return;
    const columns = process.stdout.columns || 80;
    const width = Math.max(10, Math.min(36, columns - GUTTER - 40));
    const lines = this.rows.map((row) => `\x1b[2K${this.line(row, width)}`);
    const up = this.drawn ? `\x1b[${this.drawn}A\r` : '';
    process.stdout.write(`${up}${lines.join('\n')}\n`);
    this.drawn = lines.length;
  }
}
