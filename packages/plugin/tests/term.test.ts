import { stripVTControlCharacters } from 'node:util';
import { describe, expect, it } from 'vitest';
import {
  bytes,
  crate,
  duration,
  frame,
  groupWarnings,
  packageManager,
  redraws,
  runScript,
  span,
  stack,
  wordmark,
} from '../src/term.ts';

const LIME = '\x1b[38;2;193;247;104m';

describe('wordmark', () => {
  it('is `rustrak` in half blocks, every line as wide as the others', () => {
    const lines = wordmark();
    expect(lines).toHaveLength(5);
    expect(new Set(lines.map((l) => l.length)).size).toBe(1);
    expect(lines.join('')).toMatch(/^[ ▀▄█]+$/);
  });
});

describe('frame', () => {
  const lines = wordmark();
  const plain = (frame: string[]) => frame.map(stripVTControlCharacters);

  it('shows nothing before the trace starts', () => {
    for (const line of plain(frame(lines, 0))) expect(line.trim()).toBe('');
  });

  it('reveals the mark from the left, never past the head', () => {
    const mid = plain(frame(lines, 0.3));
    const shown = mid.map((line) => line.trimEnd().length);
    const width = lines[0]!.length;
    expect(Math.max(...shown)).toBeGreaterThan(0);
    expect(Math.max(...shown)).toBeLessThan(width);
    for (const [i, line] of mid.entries()) {
      const cut = line.trimEnd().length;
      expect(line.slice(0, cut)).toBe(lines[i]!.slice(0, cut));
    }
  });

  it('ends as the whole mark, all of it lime', () => {
    const last = frame(lines, 1);
    expect(plain(last)).toEqual(lines);
    for (const line of last) {
      expect(line.replaceAll(LIME, '').replaceAll('\x1b[39m', '')).toBe(
        stripVTControlCharacters(line),
      );
    }
  });

  it('lights the head of the trace, in 256 colours and in 16 too', () => {
    for (const depth of [24, 8, 4] as const) {
      const mid = frame(lines, 0.3, depth).join('');
      expect(stripVTControlCharacters(mid).trim()).not.toBe('');
      expect(mid).toContain(depth === 4 ? '\x1b[97m' : '\x1b[38;');
    }
  });
});

describe('packageManager', () => {
  it('is whichever ran the command', () => {
    expect(packageManager('pnpm/10.0.0 npm/? node/v22.18.0')).toBe('pnpm');
    expect(packageManager('yarn/4.5.0 npm/? node/v22.18.0')).toBe('yarn');
    expect(packageManager('bun/1.2.0 npm/? node/v22.18.0')).toBe('bun');
    expect(packageManager('npm/10.9.0 node/v22.18.0')).toBe('npm');
  });

  it('is npm when nothing says otherwise', () => {
    expect(packageManager(undefined)).toBe('npm');
  });
});

describe('bytes', () => {
  it('reads like Vite: kB and MB, two decimals', () => {
    expect(bytes(512)).toBe('512 B');
    expect(bytes(145_408)).toBe('145.41 kB');
    expect(bytes(3_250_000)).toBe('3.25 MB');
  });
});

describe('crate', () => {
  it('names the crate cargo is compiling', () => {
    expect(crate('   Compiling serde_json v1.0.154')).toBe('serde_json');
    expect(
      crate('   Compiling temp-plugin v0.1.0 (/private/tmp/temp-plugin)'),
    ).toBe('temp-plugin');
  });

  it('reads a line cargo coloured', () => {
    expect(crate('\x1b[1m\x1b[92m   Compiling\x1b[0m hashbrown v0.17.2')).toBe(
      'hashbrown',
    );
  });

  it('is undefined for any other line', () => {
    expect(crate('    Updating crates.io index')).toBeUndefined();
    expect(crate('error[E0308]: mismatched types')).toBeUndefined();
  });
});

describe('span', () => {
  it('is a bar as wide as its share of the trace, where it happened', () => {
    expect(span(0, 100, 100, 10)).toBe('██████████');
    expect(span(0, 50, 100, 10)).toBe('█████     ');
    expect(span(50, 100, 100, 10)).toBe('     █████');
  });

  it('draws eighths of a cell', () => {
    expect(span(0, 55, 100, 10)).toBe('█████▌    ');
  });

  it('never vanishes: a span too short to see is a sliver', () => {
    expect(span(0, 1, 1000, 10)).toBe('▏         ');
    expect(span(999, 1000, 1000, 10)).toBe('         ▏');
  });
});

describe('stack', () => {
  it('splits the width by share, every part visible', () => {
    expect(stack([50, 30, 20], 10)).toBe('█████▓▓▓░░');
    expect(stack([98, 1, 1], 10)).toBe('████████▓░');
  });

  it('leaves out a part that is nothing', () => {
    expect(stack([60, 0, 40], 10)).toBe('██████░░░░');
  });
});

describe('duration', () => {
  it('is milliseconds under a second, tenths above', () => {
    expect(duration(12.4)).toBe('12ms');
    expect(duration(7_940)).toBe('7.9s');
  });
});

describe('groupWarnings', () => {
  it('folds the missing languages into one line each', () => {
    expect(
      groupWarnings([
        'readme/zh.md is missing: readers in zh see English',
        'readme/fr.md is missing: readers in fr see English',
        'src/messages/zh.json is missing: readers in zh see English',
        'src/messages/en.json has no settings.key.label (the key is shown)',
      ]),
    ).toEqual([
      'No readme in zh, fr: those readers see English',
      'No texts in zh: those readers see English',
      'src/messages/en.json has no settings.key.label (the key is shown)',
    ]);
  });
});

describe('runScript', () => {
  it('runs a package script, never a built-in of the same name', () => {
    expect(runScript('npm', 'build')).toBe('npm run build');
    expect(runScript('bun', 'build')).toBe('bun run build');
    expect(runScript('pnpm', 'build')).toBe('pnpm build');
    expect(runScript('yarn', 'build')).toBe('yarn build');
  });
});

describe('redraws', () => {
  const tty = { isTTY: true, columns: 100 };

  it('is a wide terminal that is not CI, dumb or read aloud', () => {
    expect(redraws({}, tty)).toBe(true);
  });

  it('is never CI, a dumb terminal, a screen reader or a pipe', () => {
    expect(redraws({ CI: 'true' }, tty)).toBe(false);
    expect(redraws({ TERM: 'dumb' }, tty)).toBe(false);
    expect(redraws({ ACCESSIBLE: '1' }, tty)).toBe(false);
    expect(redraws({}, { isTTY: false, columns: 100 })).toBe(false);
  });

  it('is never a terminal too narrow for a row to fit on one line', () => {
    expect(redraws({}, { isTTY: true, columns: 59 })).toBe(false);
    expect(redraws({}, { isTTY: true, columns: 60 })).toBe(true);
  });
});
