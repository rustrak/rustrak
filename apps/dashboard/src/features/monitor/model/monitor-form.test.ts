import type { Monitor } from '@rustrak/client';
import { describe, expect, it } from 'vitest';
import type { Translate } from '@/shared/lib/error-copy';
import {
  DEFAULT_MONITOR_FORM,
  formFromMonitor,
  inputFromForm,
  monitorFormSchema,
} from './monitor-form';

const t: Translate = (key) => key;
const schema = monitorFormSchema(t);

describe('monitorFormSchema', () => {
  it('accepts the defaults with a name', () => {
    expect(
      schema.safeParse({ ...DEFAULT_MONITOR_FORM, name: 'Backup' }).success,
    ).toBe(true);
  });

  it('requires a name', () => {
    expect(schema.safeParse(DEFAULT_MONITOR_FORM).success).toBe(false);
  });

  it('requires a crontab to be typed, leaving whether it parses to the server', () => {
    const parsed = schema.safeParse({
      ...DEFAULT_MONITOR_FORM,
      name: 'x',
      schedule: { type: 'crontab', value: '  ' },
    });
    expect(parsed.success).toBe(false);
    expect(parsed.error?.issues[0]?.path[0]).toBe('schedule');

    expect(
      schema.safeParse({
        ...DEFAULT_MONITOR_FORM,
        name: 'x',
        schedule: { type: 'crontab', value: 'not checked here' },
      }).success,
    ).toBe(true);
  });

  it('requires an interval of at least one', () => {
    expect(
      schema.safeParse({
        ...DEFAULT_MONITOR_FORM,
        name: 'x',
        schedule: { type: 'interval', value: 0, unit: 'hour' },
      }).success,
    ).toBe(false);
  });

  it('bounds the thresholds like the server', () => {
    const base = { ...DEFAULT_MONITOR_FORM, name: 'x' };
    expect(
      schema.safeParse({ ...base, failure_issue_threshold: 0 }).success,
    ).toBe(false);
    expect(schema.safeParse({ ...base, recovery_threshold: 721 }).success).toBe(
      false,
    );
  });
});

describe('inputFromForm', () => {
  it('trims and sends the schedule as typed', () => {
    const input = inputFromForm({
      ...DEFAULT_MONITOR_FORM,
      name: '  Backup ',
      schedule: { type: 'crontab', value: ' 0 3 * * * ' },
      timezone: 'Europe/Madrid',
    });
    expect(input.name).toBe('Backup');
    expect(input.config.schedule).toEqual({
      type: 'crontab',
      value: '0 3 * * *',
    });
    expect(input.config.timezone).toBe('Europe/Madrid');
  });

  it('leaves the slug out when empty so the server derives it', () => {
    expect(inputFromForm({ ...DEFAULT_MONITOR_FORM, name: 'x' }).slug).toBe(
      undefined,
    );
  });
});

describe('formFromMonitor', () => {
  it('round-trips a monitor into the form', () => {
    const config = {
      schedule: { type: 'interval' as const, value: 2, unit: 'hour' as const },
      checkin_margin: 5,
      max_runtime: 60,
      timezone: 'UTC',
      failure_issue_threshold: 3,
      recovery_threshold: 2,
    };
    const monitor: Monitor = {
      id: crypto.randomUUID(),
      slug: 'job',
      name: 'Job',
      config,
      is_muted: false,
      created_at: '2026-01-01T00:00:00Z',
      environments: [],
    };
    expect(inputFromForm(formFromMonitor(monitor)).config).toEqual(config);
  });
});
