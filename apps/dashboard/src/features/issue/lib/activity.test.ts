import type { ActivityEntry } from '@rustrak/client';
import { describe, expect, it } from 'vitest';
import { describeActivity } from './activity';

const t = (key: string, values?: Record<string, string | number>) =>
  values ? `${key} ${JSON.stringify(values)}` : key.toUpperCase();

function entry(type: string, data: unknown): ActivityEntry {
  return { type, data: JSON.stringify(data) } as ActivityEntry;
}

describe('describeActivity', () => {
  // The status entries used to hand back the message key itself, so the rail
  // read "activity.statusResolved" in every language.
  it('translates a status change', () => {
    expect(
      describeActivity(entry('set_status', { status: 'resolved' }), t),
    ).toBe('ACTIVITY.STATUSRESOLVED');
    expect(
      describeActivity(entry('set_status', { status: 'unresolved' }), t),
    ).toBe('ACTIVITY.STATUSREOPENED');
    expect(
      describeActivity(entry('set_status', { status: 'ignored' }), t),
    ).toBe('ACTIVITY.STATUSMUTED');
  });

  it('names a status it does not know', () => {
    expect(describeActivity(entry('set_status', { status: 'odd' }), t)).toBe(
      'activity.statusChanged {"status":"odd"}',
    );
  });

  it('survives data that is not JSON', () => {
    const broken = { type: 'set_priority', data: '{' } as ActivityEntry;
    expect(describeActivity(broken, t)).toBe(
      'activity.priorityChanged {"priority":"—"}',
    );
  });
});
