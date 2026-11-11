import { describe, expect, it } from 'vitest';
import { CRON_GUIDES, cronCheckInUrl, cronGuide, parseDsn } from './setup';

const dsn = 'https://0123456789abcdef0123456789abcdef@errors.example.com/42';

describe('parseDsn', () => {
  it('splits a DSN into origin, key and project', () => {
    expect(parseDsn(dsn)).toEqual({
      origin: 'https://errors.example.com',
      key: '0123456789abcdef0123456789abcdef',
      projectId: '42',
    });
  });

  it('keeps a port and a path prefix', () => {
    expect(parseDsn('http://k@localhost:8080/sub/7')).toEqual({
      origin: 'http://localhost:8080/sub',
      key: 'k',
      projectId: '7',
    });
  });

  it('is null for something that is not a DSN', () => {
    expect(parseDsn('not a dsn')).toBeNull();
    expect(parseDsn('https://errors.example.com/42')).toBeNull();
  });
});

describe('cronCheckInUrl', () => {
  it('is the HTTP check-in endpoint with the key in the path', () => {
    expect(cronCheckInUrl(dsn, 'nightly-backup')).toBe(
      'https://errors.example.com/api/42/cron/nightly-backup/0123456789abcdef0123456789abcdef/',
    );
  });
});

describe('cronGuide', () => {
  it('fills the slug into every guide', () => {
    for (const id of CRON_GUIDES) {
      expect(cronGuide(id, { dsn, slug: 'nightly-backup' }).code).toContain(
        'nightly-backup',
      );
    }
  });

  it('curl reports in_progress, ok and error against the endpoint', () => {
    const { code, language } = cronGuide('curl', { dsn, slug: 'job' });
    expect(language).toBe('bash');
    expect(code).toContain('/api/42/cron/job/');
    expect(code).toContain('?status=in_progress');
    expect(code).toContain('?status=ok');
    expect(code).toContain('?status=error');
    // One final status per run: the snippet runs top to bottom when pasted.
    expect(code).toMatch(
      /if [\s\S]*then[\s\S]*status=ok[\s\S]*else[\s\S]*status=error[\s\S]*fi/,
    );
    // A failed task still fails the script, for a scheduler that reads it.
    expect(code).toMatch(/else\s+status=\$\?[\s\S]*exit "\$status"\s+fi/);
  });

  it('upserts the monitor when a schedule is given', () => {
    const schedule = { type: 'crontab' as const, value: '0 3 * * *' };
    expect(cronGuide('curl', { dsn, slug: 'job', schedule }).code).toContain(
      '"schedule": {"type": "crontab", "value": "0 3 * * *"}',
    );
    expect(cronGuide('cli', { dsn, slug: 'job', schedule }).code).toContain(
      '--schedule "0 3 * * *"',
    );
    expect(cronGuide('python', { dsn, slug: 'job', schedule }).code).toContain(
      'monitor_config=monitor_config',
    );
  });

  it('writes an interval schedule in each tool own shape', () => {
    const schedule = {
      type: 'interval' as const,
      value: 10,
      unit: 'minute' as const,
    };
    expect(cronGuide('node', { dsn, slug: 'job', schedule }).code).toMatch(
      /type: "interval",\s*value: 10,\s*unit: "minute",/,
    );
  });

  it('puts the DSN where the tool reads it', () => {
    expect(cronGuide('cli', { dsn, slug: 'job' }).code).toContain(
      `export SENTRY_DSN=${dsn}`,
    );
  });
});
