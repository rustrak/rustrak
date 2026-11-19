import { describe, expect, it } from 'vitest';
import { allPages } from './pages';

/** A span list of `rows` rows whose count stops at `cap`, as the server's does. */
function listOf(rows: number, cap: number, perPage: number) {
  const asked: number[] = [];
  const page = async (n: number) => {
    asked.push(n);
    const counted = Math.min(rows, cap);
    return {
      items: Array.from(
        { length: Math.max(Math.min(perPage, rows - (n - 1) * perPage), 0) },
        (_, i) => (n - 1) * perPage + i,
      ),
      total_pages: Math.ceil(counted / perPage),
      total_capped: rows > cap,
      per_page: perPage,
    };
  };
  return { page, asked };
}

describe('allPages', () => {
  it('reads the pages the count names', async () => {
    const { page, asked } = listOf(25, 1000, 10);
    expect(await allPages(page)).toHaveLength(25);
    expect(asked).toEqual([1, 2, 3]);
  });

  it('reads one page when that is all there is', async () => {
    const { page, asked } = listOf(3, 1000, 10);
    expect(await allPages(page)).toEqual([0, 1, 2]);
    expect(asked).toEqual([1]);
  });

  it('reads past a capped count until a page comes back short', async () => {
    const { page, asked } = listOf(45, 20, 10);
    expect(await allPages(page)).toEqual(
      Array.from({ length: 45 }, (_, i) => i),
    );
    expect(asked).toEqual([1, 2, 3, 4, 5]);
  });

  it('stops on an empty page when the rows end on a full one', async () => {
    const { page, asked } = listOf(40, 20, 10);
    expect(await allPages(page)).toHaveLength(40);
    expect(asked).toEqual([1, 2, 3, 4, 5]);
  });
});
