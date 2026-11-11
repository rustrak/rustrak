import type { DataTableQuery } from '@rustrak/ui';
import { describe, expect, it } from 'vitest';
import { issueListParams, withServerQuery } from './issue-table';

const base: DataTableQuery = {
  sorting: [],
  filters: [],
  search: '',
  pagination: { pageIndex: 0, pageSize: 50 },
};

describe('issueListParams', () => {
  it('reads the open view with nothing else as the plain list', () => {
    expect(issueListParams(base, 'open')).toEqual({
      filter: 'open',
      page: 1,
      per_page: 50,
      sort: 'last_seen',
      order: 'desc',
    });
  });

  it('turns each view into its status', () => {
    expect(issueListParams(base, 'muted')).toMatchObject({ filter: 'muted' });
    expect(issueListParams(base, 'all')).toMatchObject({ filter: 'all' });
  });

  it('sends every filter in the shape the API reads', () => {
    const params = issueListParams(
      {
        ...base,
        search: 'timeout',
        filters: [
          { id: 'level', value: ['fatal', 'error'] },
          { id: 'priority', value: ['high'] },
          { id: 'lastSeen', value: ['24h'] },
          { id: 'events', value: [100, null] },
          { id: 'environment', value: ['production'] },
        ],
        sorting: [{ id: 'events', desc: false }],
        pagination: { pageIndex: 2, pageSize: 25 },
      },
      'all',
    );

    expect(params).toEqual({
      filter: 'all',
      q: 'timeout',
      level: 'fatal,error',
      priority: 'high',
      last_seen: '24h',
      events_min: 100,
      environment: 'production',
      sort: 'event_count',
      order: 'asc',
      page: 3,
      per_page: 25,
    });
  });

  it('sends the assignee filter as `assigned`', () => {
    expect(
      issueListParams(
        { ...base, filters: [{ id: 'assignee', value: ['none'] }] },
        'open',
      ),
    ).toMatchObject({ assigned: 'none' });
  });
});

describe('withServerQuery', () => {
  it('keeps the one sort the server applies and drops any other', () => {
    const query: DataTableQuery = {
      ...base,
      sorting: [
        { id: 'nonsense', desc: false },
        // An inherited key is not a sort: `toString` is on every object.
        { id: 'toString', desc: false },
        { id: 'events', desc: true },
        { id: 'priority', desc: false },
      ],
    };
    expect(withServerQuery(query).sorting).toEqual([
      { id: 'events', desc: true },
    ]);
  });

  it('keeps only the seen windows the server reads', () => {
    const query: DataTableQuery = {
      ...base,
      filters: [
        { id: 'lastSeen', value: ['tomorrow'] },
        { id: 'firstSeen', value: ['24h'] },
        { id: 'level', value: ['error'] },
      ],
    };
    expect(withServerQuery(query).filters).toEqual([
      { id: 'firstSeen', value: ['24h'] },
      { id: 'level', value: ['error'] },
    ]);
  });
});
