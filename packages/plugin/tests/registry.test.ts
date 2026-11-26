// `rustrak-plugin index <dir>`: the registry's index.json from the packages
// in a folder. Signing it (`minisign -Sm index.json`) is the registry's CI's.

import { createHash } from 'node:crypto';
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { strToU8, zipSync } from 'fflate';
import { describe, expect, it } from 'vitest';
import { writeIndex } from '../src/registry.ts';

const manifest = (version: string, rustrak: string, id = 'hello') => `[plugin]
id = "${id}"
name = "Hello"
version = "${version}"
description = "The smallest Rustrak plugin."
rustrak = "${rustrak}"
world = "0.1.0"
author = "Rustrak"
icon = "star"

[capabilities]
sql = true
`;

const LOGO = "<svg xmlns='http://www.w3.org/2000/svg'/>";

function rtp(
  dir: string,
  version: string,
  rustrak: string,
  id?: string,
): Uint8Array {
  const bytes = zipSync({
    'rustrak-plugin.toml': strToU8(manifest(version, rustrak, id)),
    'logo.svg': strToU8(LOGO),
  });
  writeFileSync(join(dir, `hello-${version}.rtp`), bytes);
  return bytes;
}

describe('writeIndex', () => {
  it('lists every package by plugin, each version with its url and sha256', () => {
    const dir = mkdtempSync(join(tmpdir(), 'rustrak-registry-'));
    const one = rtp(dir, '0.1.0', '>=0.33.0');
    rtp(dir, '0.2.0', '>=0.34.0');
    writeIndex(dir);
    const index = JSON.parse(readFileSync(join(dir, 'index.json'), 'utf8'));
    expect(index.version).toBe(1);
    expect(index.plugins).toHaveLength(1);
    const hello = index.plugins[0];
    expect(hello).toMatchObject({
      id: 'hello',
      name: 'Hello',
      description: 'The smallest Rustrak plugin.',
      icon: 'star',
      author: 'Rustrak',
      // Its logo beside the index, named by the plugin's id.
      logo: {
        url: 'hello.svg',
        sha256: createHash('sha256').update(LOGO).digest('hex'),
      },
    });
    expect(readFileSync(join(dir, 'hello.svg'), 'utf8')).toBe(LOGO);
    expect(hello.versions).toEqual([
      {
        version: '0.1.0',
        rustrak: '>=0.33.0',
        url: 'hello-0.1.0.rtp',
        sha256: createHash('sha256').update(one).digest('hex'),
      },
      expect.objectContaining({ version: '0.2.0', url: 'hello-0.2.0.rtp' }),
    ]);
  });

  it('orders a pre-release before its release', () => {
    const dir = mkdtempSync(join(tmpdir(), 'rustrak-registry-'));
    rtp(dir, '1.0.0', '>=0.33.0');
    rtp(dir, '1.0.0-rc.2', '>=0.33.0');
    rtp(dir, '1.0.0-rc.10', '>=0.33.0');
    writeIndex(dir);
    const index = JSON.parse(readFileSync(join(dir, 'index.json'), 'utf8'));
    expect(
      index.plugins[0].versions.map((v: { version: string }) => v.version),
    ).toEqual(['1.0.0-rc.2', '1.0.0-rc.10', '1.0.0']);
  });

  it('orders pre-releases by semver identifiers, ignoring build metadata', () => {
    const dir = mkdtempSync(join(tmpdir(), 'rustrak-registry-'));
    for (const v of [
      '1.0.10+build',
      '1.0.2',
      '1.0.0-alpha-10',
      '1.0.0-alpha-2',
    ])
      rtp(dir, v, '>=0.33.0');
    writeIndex(dir);
    const index = JSON.parse(readFileSync(join(dir, 'index.json'), 'utf8'));
    expect(
      index.plugins[0].versions.map((v: { version: string }) => v.version),
    ).toEqual(['1.0.0-alpha-10', '1.0.0-alpha-2', '1.0.2', '1.0.10+build']);
  });

  it('refuses a package whose id is not a plugin id, before writing anything', () => {
    const dir = mkdtempSync(join(tmpdir(), 'rustrak-registry-'));
    rtp(dir, '0.1.0', '>=0.33.0', '../victim');
    expect(() => writeIndex(dir)).toThrow('"../victim" is not a plugin id');
    expect(existsSync(join(dir, '..', 'victim.svg'))).toBe(false);
  });
});
