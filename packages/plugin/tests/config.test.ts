import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { defineConfig, loadConfig } from '../src/config.ts';

function project(files: Record<string, string>): string {
  const dir = mkdtempSync(join(tmpdir(), 'rustrak-plugin-config-'));
  for (const [name, text] of Object.entries(files))
    writeFileSync(join(dir, name), text);
  return dir;
}

describe('loadConfig', () => {
  it('reads rustrak.config.ts', async () => {
    const dir = project({
      'rustrak.config.ts': `const name: string = 'Typed';\nexport default { name, rustrak: '>=0.33.0' };\n`,
    });
    expect(await loadConfig(dir)).toEqual({
      name: 'Typed',
      rustrak: '>=0.33.0',
    });
  });

  it('says where it looked when there is none', async () => {
    await expect(loadConfig(project({}))).rejects.toThrow(
      /rustrak\.config\.ts/,
    );
  });
});

// Checked by `tsc` (check-types): each line must be refused by the types.
describe('defineConfig types', () => {
  it('refuses what Rustrak would not install', () => {
    const base = { name: 'X', rustrak: '>=0.34.0' } as const;
    defineConfig({
      ...base,
      settings: {
        // biome-ignore format: one line, for the directive above it
        // @ts-expect-error a select's default is one of its options
        mode: { type: 'select', options: ['daily', 'weekly'], default: 'hourly' },
      },
    });
    defineConfig({
      ...base,
      // @ts-expect-error an unknown setting type
      settings: { a: { type: 'colour' } },
    });
    defineConfig({
      ...base,
      // @ts-expect-error a secret has no default and no project scope
      settings: { a: { type: 'secret', scope: 'project' } },
    });
    defineConfig({
      ...base,
      // @ts-expect-error a permission needs an action, from the known ones
      permissions: { hits: { scope: 'project', actions: [] } },
    });
    defineConfig({
      ...base,
      // @ts-expect-error scope is instance or project
      permissions: { hits: { scope: 'global', actions: ['read'] } },
    });
    // @ts-expect-error rustrak is a semver range
    defineConfig({ name: 'X', rustrak: 'latest' });
    // @ts-expect-error an icon of @rustrak/ui
    defineConfig({ ...base, icon: 'unicorn' });
    // And the same, right, compiles.
    defineConfig({
      ...base,
      settings: {
        mode: {
          type: 'select',
          options: ['daily', 'weekly'],
          default: 'weekly',
        },
      },
    });
  });
});
