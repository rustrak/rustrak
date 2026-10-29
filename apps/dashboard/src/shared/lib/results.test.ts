import type { RustrakError } from '@rustrak/client';
import { Err, Ok } from '@rustrak/client';
import { describe, expect, it } from 'vitest';
import { firstFailure, okOrNull, orElse } from './results';

const down: RustrakError = {
  kind: 'network',
  reason: 'timeout',
  message: 'The request timed out.',
};

describe('orElse', () => {
  it('keeps the data of a read that answered', async () => {
    expect(await orElse(Promise.resolve(Ok([1])), [])).toEqual(Ok([1]));
  });

  it('turns a failed read the page can do without into its fallback', async () => {
    expect(await orElse(Promise.resolve(Err(down)), [])).toEqual(Ok([]));
  });
});

describe('firstFailure', () => {
  it('skips the reads that were never made', () => {
    expect(firstFailure([Ok(1), null, Err(down)])).toBe(down);
    expect(firstFailure([Ok(1), null])).toBeUndefined();
  });
});

describe('okOrNull', () => {
  it('is the data, or null for a failure or no read', () => {
    expect(okOrNull(Ok(2))).toBe(2);
    expect(okOrNull(Err(down))).toBeNull();
    expect(okOrNull(null)).toBeNull();
  });
});
