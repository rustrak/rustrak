import { describe, expect, it } from 'vitest';
import { droppedToFit } from './fit';

const columns = [
  { id: 'title', width: 320 },
  { id: 'trend', width: 120, drop: 1 },
  { id: 'events', width: 120 },
  { id: 'age', width: 128, drop: 2 },
];

describe('droppedToFit', () => {
  it('drops nothing while the table fits, or before it is measured', () => {
    expect(droppedToFit(columns, 688)).toEqual(new Set());
    expect(droppedToFit(columns, undefined)).toEqual(new Set());
  });

  it('drops the optional columns in their order until the rest fit', () => {
    expect(droppedToFit(columns, 600)).toEqual(new Set(['trend']));
    expect(droppedToFit(columns, 500)).toEqual(new Set(['trend', 'age']));
  });

  it('never drops a column that is not optional', () => {
    expect(droppedToFit(columns, 100)).toEqual(new Set(['trend', 'age']));
  });
});
