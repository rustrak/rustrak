import { describe, expect, it } from 'vitest';
import { ownRoles } from './project-members';

const own = { id: 1, name: 'here', inherited: false };
const inherited = { id: 2, name: 'everywhere', inherited: true };

describe('ownRoles', () => {
  it('keeps the roles held on this project alone', () => {
    expect(ownRoles({ roles: [own, inherited] })).toEqual([own]);
  });

  it('is empty for who only inherits: nothing here can be taken away', () => {
    expect(ownRoles({ roles: [inherited] })).toEqual([]);
  });
});
