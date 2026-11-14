import type { SavedView } from '@rustrak/client';
import { describe, expect, it } from 'vitest';
import {
  activeView,
  bareSearch,
  defaultView,
  searchOf,
  viewMenu,
  viewQuery,
} from './saved-view';

function view(over: Partial<SavedView>): SavedView {
  return {
    id: 1,
    project_id: 1,
    created_by: 1,
    page: 'issues',
    name: 'v',
    query: '',
    visibility: 'owner',
    is_project_default: false,
    is_my_default: false,
    created_at: '2026-11-14T00:00:00Z',
    ...over,
  };
}

describe('viewQuery', () => {
  it('writes the same address the same way, whatever order it came in', () => {
    expect(viewQuery({ view: 'all', q: 'level:fatal' })).toBe(
      viewQuery({ q: 'level:fatal', view: 'all' }),
    );
  });

  it('leaves out what is unset and which page of results was open', () => {
    expect(
      viewQuery({ q: 'level:fatal', sort: undefined, release: '', page: 3 }),
    ).toBe('q=level%3Afatal');
  });

  it('reads back as the search it was written from, as text', () => {
    expect(searchOf(viewQuery({ q: 'a b', compare: true }))).toEqual({
      compare: 'true',
      q: 'a b',
    });
  });
});

describe('activeView', () => {
  it('is the view whose address is the one on screen', () => {
    const views = [
      view({ id: 1, query: 'q=a' }),
      view({ id: 2, query: 'q=b' }),
    ];
    expect(activeView(views, { q: 'b', page: 2 })?.id).toBe(2);
    expect(activeView(views, { q: 'c' })).toBeUndefined();
  });
});

describe('activeView, when two views hold the same address', () => {
  const first = view({ id: 1, query: 'q=a' });
  const second = view({ id: 2, query: 'q=a' });

  it('is the one the reader opened', () => {
    expect(activeView([first, second], { q: 'a' }, 2)?.id).toBe(2);
  });

  it('is the first when the opened one no longer matches', () => {
    expect(activeView([first, second], { q: 'a' }, 3)?.id).toBe(1);
  });

  it('reads a query saved by hand the way the address writes it', () => {
    const byHand = view({ id: 3, query: 'view=all&q=level:fatal' });
    expect(activeView([byHand], { q: 'level:fatal', view: 'all' })?.id).toBe(3);
  });
});

describe('bareSearch', () => {
  it('is an address with nothing set', () => {
    expect(bareSearch({ q: undefined, sort: '' })).toBe(true);
  });

  it('is not one that asks for a page or an open line', () => {
    expect(bareSearch({ page: 2 })).toBe(false);
    expect(bareSearch({ selected: 'abc' })).toBe(false);
  });
});

describe('defaultView', () => {
  it('is mine before the project’s', () => {
    const project = view({ id: 1, is_project_default: true });
    const mine = view({ id: 2, is_my_default: true });
    expect(defaultView([project, mine])?.id).toBe(2);
    expect(defaultView([project])?.id).toBe(1);
    expect(defaultView([view({})])).toBeUndefined();
  });
});

describe('viewMenu', () => {
  const fatal = view({
    id: 1,
    name: 'Fatal',
    query: 'q=level%3Afatal',
    visibility: 'project',
    created_by: 2,
  });
  const mine = view({ id: 2, name: 'Mine', query: 'q=me', created_by: 1 });
  const views = [fatal, mine];
  const base = { views, me: 1, isAdmin: false, editing: undefined };
  const kinds = (menu: ReturnType<typeof viewMenu>) =>
    menu.actions.map((a) => a.kind);

  it('on a view, offers to make it a default and, if it is mine, to delete it', () => {
    const menu = viewMenu({ ...base, search: { q: 'me' } });
    expect(menu.current?.id).toBe(2);
    expect(menu.edited).toBe(false);
    expect(kinds(menu)).toEqual(['myDefault', 'delete']);
  });

  it('never lets a non-admin change someone else’s shared view', () => {
    const menu = viewMenu({ ...base, search: { q: 'level:fatal' } });
    expect(kinds(menu)).toEqual(['myDefault']);
    const admin = viewMenu({
      ...base,
      isAdmin: true,
      search: { q: 'level:fatal' },
    });
    expect(kinds(admin)).toEqual(['myDefault', 'projectDefault', 'delete']);
  });

  it('keeps an opened view as the one being edited once its filters change', () => {
    const menu = viewMenu({ ...base, editing: 2, search: { q: 'me other' } });
    expect(menu.current?.id).toBe(2);
    expect(menu.edited).toBe(true);
    expect(kinds(menu)).toEqual(['save', 'saveAs', 'reset']);
    const theirs = viewMenu({ ...base, editing: 1, search: { q: 'x' } });
    expect(kinds(theirs)).toEqual(['saveAs', 'reset']);
  });

  it('offers to save filters no view holds, and nothing for a bare page', () => {
    expect(kinds(viewMenu({ ...base, search: { q: 'new' } }))).toEqual([
      'saveAs',
    ]);
    expect(kinds(viewMenu({ ...base, search: {} }))).toEqual([]);
  });
});
