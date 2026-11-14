import { describe, expect, it } from 'vitest';
import { assignable, everyone } from './assignable';

describe('assignable', () => {
  it('adds the reader when they are not a member', () => {
    expect(assignable([], { id: 1, email: 'admin@x.io' })).toEqual([
      { id: 1, email: 'admin@x.io' },
    ]);
  });

  it('does not list the reader twice', () => {
    expect(
      assignable([{ user_id: 1, email: 'admin@x.io' }], {
        id: 1,
        email: 'admin@x.io',
      }),
    ).toHaveLength(1);
  });
});

describe('everyone', () => {
  it('joins several projects members, each person once', () => {
    expect(
      everyone([
        [
          { id: 1, email: 'a@x' },
          { id: 2, email: 'b@x' },
        ],
        [
          { id: 2, email: 'b@x' },
          { id: 3, email: 'c@x' },
        ],
      ]),
    ).toEqual([
      { id: 1, email: 'a@x' },
      { id: 2, email: 'b@x' },
      { id: 3, email: 'c@x' },
    ]);
  });
});
