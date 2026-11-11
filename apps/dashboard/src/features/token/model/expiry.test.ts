import { describe, expect, it } from 'vitest';
import { tokenExpiry } from './expiry';

const now = new Date(2026, 10, 9, 12, 0, 0);

describe('tokenExpiry', () => {
  it('reads an empty field as a token that never expires', () => {
    expect(tokenExpiry('', now)).toEqual({ ok: true, value: null });
  });

  it('expires at the end of the chosen day, today included', () => {
    expect(tokenExpiry('2026-11-09', now)).toEqual({
      ok: true,
      value: new Date(2026, 10, 9, 23, 59, 59).toISOString(),
    });
  });

  it('rejects a day already gone', () => {
    expect(tokenExpiry('2026-11-08', now)).toEqual({ ok: false });
  });
});
