import { describe, expect, it } from 'vitest';
import type { Settings } from '../src/config.ts';
import { settingsRust, settingsTs } from '../src/settings.ts';

const settings: Settings = {
  greeting: { type: 'text', default: 'Hello' },
  apiKey: { type: 'secret', required: true },
  retentionDays: {
    type: 'number',
    scope: 'project',
    integer: true,
    default: 30,
    min: 1,
    max: 365,
  },
  sampleRate: { type: 'number', min: 0, max: 1 },
  enabled: { type: 'boolean', default: true, scope: 'project' },
  mode: {
    type: 'select',
    options: ['simple', 'full-funnel'],
    default: 'simple',
  },
  endpoint: { type: 'url' },
};

describe('settingsTs', () => {
  it('types what the browser gets: no secrets, null where nothing is set', () => {
    const ts = settingsTs(settings);
    expect(ts).toContain('greeting: string;');
    expect(ts).toContain('retentionDays: number;');
    expect(ts).toContain('sampleRate: number | null;');
    expect(ts).toContain('enabled: boolean;');
    expect(ts).toContain("mode: 'simple' | 'full-funnel';");
    expect(ts).toContain('endpoint: string | null;');
    expect(ts).not.toContain('apiKey');
  });
});

describe('settingsRust', () => {
  it('types what the server gets, secrets included', () => {
    const rs = settingsRust(settings);
    expect(rs).toContain('pub struct Settings {');
    expect(rs).toContain('pub greeting: String,');
    expect(rs).toContain('pub api_key: String,');
    expect(rs).toContain('pub retention_days: i64,');
    expect(rs).toContain('pub sample_rate: Option<f64>,');
    expect(rs).toContain('pub enabled: bool,');
    expect(rs).toContain('pub mode: Mode,');
    expect(rs).toContain('pub endpoint: Option<String>,');
    expect(rs).toContain('pub enum Mode {');
    expect(rs).toContain('#[serde(rename = "full-funnel")]\n    FullFunnel,');
  });

  it('is an empty struct for a plugin with no settings', () => {
    expect(settingsRust({})).toContain('pub struct Settings {}');
  });
});
