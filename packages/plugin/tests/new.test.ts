import { mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import pkg from '../package.json' with { type: 'json' };
import { nameError, scaffold } from '../src/new.ts';

describe('scaffold', () => {
  const dir = join(
    mkdtempSync(join(tmpdir(), 'rustrak-plugin-new-')),
    'visits',
  );
  scaffold(dir);
  const read = (file: string) => readFileSync(join(dir, file), 'utf8');

  it('depends on the crates.io SDK of this release', () => {
    expect(read('Cargo.toml')).toContain(`rustrak-plugin = "${pkg.version}"`);
  });

  it('runs on the first Rustrak with plugins', () => {
    expect(read('rustrak.config.ts')).toContain(`rustrak: '>=0.34.0'`);
  });
});

describe('nameError', () => {
  const parent = mkdtempSync(join(tmpdir(), 'rustrak-plugin-name-'));

  it('accepts a free directory with a plugin name', () => {
    expect(nameError(join(parent, 'visits'))).toBeUndefined();
  });

  it('says what a name is', () => {
    expect(nameError(join(parent, 'Visits'))).toBe(
      'Visits: a plugin name is 2 to 32 lowercase letters, digits or dashes',
    );
  });

  it('refuses a directory that has files', () => {
    const taken = join(parent, 'taken');
    scaffold(taken);
    expect(nameError(taken)).toBe(`${taken} is not empty`);
  });
});
