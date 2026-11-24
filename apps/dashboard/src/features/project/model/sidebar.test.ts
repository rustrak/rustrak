import type { SidebarProjects } from '@rustrak/client';
import { describe, expect, it } from 'vitest';
import {
  afterFailedSave,
  inManualOrder,
  makeRoom,
  moveTo,
  sidebarProjects,
} from './sidebar';

const p = (id: number, name: string) => ({ id, name });
// Newest first, as the project list comes.
const projects = [p(4, 'delta'), p(3, 'Charlie'), p(2, 'bravo'), p(1, 'alpha')];
const ids = (list: { id: number }[]) => list.map((x) => x.id);

describe('inManualOrder', () => {
  it('puts the saved order first and the rest after, as they came', () => {
    expect(ids(inManualOrder(projects, [2, 4]))).toEqual([2, 4, 3, 1]);
  });

  it('skips ids of projects that are gone', () => {
    expect(ids(inManualOrder(projects, [99, 1]))).toEqual([1, 4, 3, 2]);
  });
});

describe('sidebarProjects', () => {
  it('lists every project when nothing is saved', () => {
    const prefs = {
      sort: 'manual',
      order: [],
      hidden: [],
    } satisfies SidebarProjects;
    expect(ids(sidebarProjects(projects, prefs))).toEqual([4, 3, 2, 1]);
  });

  it('leaves out the hidden ones, in the manual order', () => {
    const prefs = {
      sort: 'manual',
      order: [1, 2],
      hidden: [2, 3],
    } satisfies SidebarProjects;
    expect(ids(sidebarProjects(projects, prefs))).toEqual([1, 4]);
  });

  it('sorts by name regardless of case', () => {
    const prefs = {
      sort: 'name',
      order: [4],
      hidden: [],
    } satisfies SidebarProjects;
    expect(ids(sidebarProjects(projects, prefs))).toEqual([1, 2, 3, 4]);
  });

  it('keeps the order it was given for activity', () => {
    const prefs = {
      sort: 'activity',
      order: [1],
      hidden: [4],
    } satisfies SidebarProjects;
    expect(ids(sidebarProjects(projects, prefs))).toEqual([3, 2, 1]);
  });
});

describe('moveTo', () => {
  it('moves an id to a new index', () => {
    expect(moveTo([1, 2, 3, 4], 4, 1)).toEqual([1, 4, 2, 3]);
  });

  it('clamps past either end', () => {
    expect(moveTo([1, 2, 3], 1, 9)).toEqual([2, 3, 1]);
    expect(moveTo([1, 2, 3], 3, -1)).toEqual([3, 1, 2]);
  });
});

describe('makeRoom', () => {
  // Dragging row 1 down to index 3: rows 2 and 3 slide up a place.
  it('slides the rows between up when dragging down', () => {
    expect([0, 1, 2, 3, 4].map((i) => makeRoom(i, 1, 3))).toEqual([
      0, 0, -1, -1, 0,
    ]);
  });

  it('slides the rows between down when dragging up', () => {
    expect([0, 1, 2, 3, 4].map((i) => makeRoom(i, 3, 1))).toEqual([
      0, 1, 1, 0, 0,
    ]);
  });

  it('moves nothing while the row is over its own place', () => {
    expect([0, 1, 2].map((i) => makeRoom(i, 1, 1))).toEqual([0, 0, 0]);
  });
});

describe('afterFailedSave', () => {
  const confirmed = {
    sort: 'manual',
    order: [2, 1],
    hidden: [],
  } satisfies SidebarProjects;
  const failed = { ...confirmed, hidden: [1] };

  it('goes back to what the server confirmed when nothing newer was made', () => {
    expect(afterFailedSave(failed, failed, confirmed)).toBe(confirmed);
  });

  it('keeps a newer change: its own save carries this one too', () => {
    const newer = { ...failed, sort: 'name' as const };
    expect(afterFailedSave(newer, failed, confirmed)).toBe(newer);
  });

  it('keeps what it shows when nothing was ever confirmed', () => {
    expect(afterFailedSave(failed, failed, undefined)).toBe(failed);
  });
});
