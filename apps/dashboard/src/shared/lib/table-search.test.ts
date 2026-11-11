import type { DataTableQuery } from '@rustrak/ui';
import { describe, expect, it } from 'vitest';
import { filterValue, pageBeyondEnd, tableSearch } from './table-search';

const codec = tableSearch({
  variants: { environment: 'options' },
  pageSize: 25,
  // An API that ranks by two columns only, always from the most down.
  normalise: (query: DataTableQuery) => ({
    ...query,
    sorting: query.sorting
      .filter((s) => s.id === 'count' || s.id === 'time')
      .map((s) => ({ ...s, desc: true })),
  }),
  defaultSort: { id: 'time', desc: true },
});

describe('tableSearch', () => {
  it('keeps a valid address as it is', () => {
    const search = { q: 'environment:prod api', sort: '-count', page: 3 };
    expect(codec.validate(search)).toEqual({ ...search, per: undefined });
  });

  it('drops what the table would not read', () => {
    expect(
      codec.validate({ sort: '-nonsense', page: 'abc', per: '0', q: '' }),
    ).toEqual({
      q: undefined,
      sort: undefined,
      page: undefined,
      per: undefined,
    });
  });

  it('falls back to its own page size past what the API serves', () => {
    expect(codec.read({ per: 101 }).pagination.pageSize).toBe(25);
    expect(codec.read({ per: 100 }).pagination.pageSize).toBe(100);
  });

  it('reads the page the router hands as a number', () => {
    expect(codec.read(codec.validate({ page: 2 })).pagination.pageIndex).toBe(
      1,
    );
  });

  it('shows the default sort without writing it to the address', () => {
    const query = codec.read({});
    expect(query.sorting).toEqual([{ id: 'time', desc: true }]);
    expect(codec.write(query).sort).toBeUndefined();
  });

  it('round-trips a change', () => {
    const query = codec.read({ q: 'environment:prod' });
    const next = codec.write({
      ...query,
      sorting: [{ id: 'count', desc: false }],
      pagination: { ...query.pagination, pageIndex: 1 },
    });
    expect(next).toEqual({
      q: 'environment:prod',
      sort: '-count',
      page: 2,
      per: undefined,
    });
  });
});

describe('tableSearch.apply', () => {
  it('applies each change to the address it is given, so two in a row both land', () => {
    const toPage2 = (q: DataTableQuery) => ({
      ...q,
      pagination: { ...q.pagination, pageIndex: 1 },
    });
    const byCount = (q: DataTableQuery) => ({
      ...q,
      sorting: [{ id: 'count', desc: true }],
    });
    const first = codec.apply(toPage2, { q: 'environment:prod' });
    expect(codec.apply(byCount, { q: 'environment:prod', ...first })).toEqual({
      q: 'environment:prod',
      sort: '-count',
      page: 2,
      per: undefined,
    });
  });
});

describe('filterValue', () => {
  it('takes the first option or the text', () => {
    const query = codec.read({ q: 'environment:prod,staging' });
    expect(filterValue(query, 'environment')).toBe('prod');
    expect(filterValue(query, 'missing')).toBeUndefined();
  });
});

describe('pageBeyondEnd', () => {
  it('answers the last page for an address past it', () => {
    expect(pageBeyondEnd(9, 3)).toBe(3);
    // Nothing left at all: back to the first page, written as no page.
    expect(pageBeyondEnd(2, 0)).toBeUndefined();
  });

  it('answers false for a page that exists', () => {
    expect(pageBeyondEnd(3, 3)).toBe(false);
    expect(pageBeyondEnd(1, 0)).toBe(false);
  });
});
