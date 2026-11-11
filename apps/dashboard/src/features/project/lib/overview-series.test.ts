import { describe, expect, it } from 'vitest';
import { withPreviousPeriod } from './overview-series';

describe('withPreviousPeriod', () => {
  // A doubled window, zero-filled: 2P/interval + 1 buckets, oldest first.
  const point = (total: number) => ({ total });

  it('pairs each current bucket with the one a period before', () => {
    const doubled = [1, 2, 3, 10, 20, 30, 40].map(point);
    // The current window starts mid-bucket, so its first bucket holds less
    // than the doubled series' bucket of the same hour: it stays as fetched.
    const current = [4, 20, 30, 40].map(point);
    expect(withPreviousPeriod(current, doubled)).toEqual([
      { total: 4, previous: 1 },
      { total: 20, previous: 2 },
      { total: 30, previous: 3 },
      { total: 40, previous: 10 },
    ]);
  });

  it('is empty for an empty series', () => {
    expect(withPreviousPeriod([], [])).toEqual([]);
  });
});
