import { describe, expect, it } from 'vitest';
import { parseRateLimit } from './rate-limit';

describe('parseRateLimit', () => {
  it('reads an empty field as no limit of its own', () => {
    expect(parseRateLimit('')).toEqual({ ok: true, value: null });
    expect(parseRateLimit('   ')).toEqual({ ok: true, value: null });
  });

  it('reads a whole number of at least one as the limit', () => {
    expect(parseRateLimit('300')).toEqual({ ok: true, value: 300 });
    expect(parseRateLimit(' 1 ')).toEqual({ ok: true, value: 1 });
  });

  it('rejects what the server would reject', () => {
    // 9007199254740993 would round to ...992 and save a different limit.
    for (const raw of ['0', '-5', '2.5', 'abc', '1e3', '9007199254740993']) {
      expect(parseRateLimit(raw).ok).toBe(false);
    }
  });
});
