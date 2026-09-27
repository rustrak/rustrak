import { describe, expect, it } from 'vitest';
import { isNonEmpty } from './event-payload';

describe('isNonEmpty', () => {
  it('is true for a record with keys', () => {
    expect(isNonEmpty({ react: '19.0.0' })).toBe(true);
  });

  it('is false for an empty or missing record', () => {
    expect(isNonEmpty({})).toBe(false);
    expect(isNonEmpty(undefined)).toBe(false);
  });

  it('is false for a record the SDK sent as null', () => {
    const payload = JSON.parse('{"extra": null}') as {
      extra?: Record<string, unknown>;
    };
    expect(isNonEmpty(payload.extra)).toBe(false);
  });
});
