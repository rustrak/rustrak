import { describe, expect, it } from 'vitest';
import { providerEditing } from './provider-view';

const one = { id: 3 };
const all = { create: true, update: true };

describe('providerEditing', () => {
  it('opens on the form when there is nothing yet and one can be added', () => {
    expect(providerEditing(undefined, [], all)).toBe('new');
  });

  it('opens on the list for who cannot add one', () => {
    const readOnly = { create: false, update: false };
    expect(providerEditing(undefined, [], readOnly)).toBeNull();
    expect(providerEditing('new', [one], readOnly)).toBeNull();
  });

  it('opens a stored one only for who may change it', () => {
    expect(providerEditing(3, [one], all)).toBe(one);
    expect(
      providerEditing(3, [one], { create: true, update: false }),
    ).toBeNull();
  });

  it('treats an id that is gone as the list', () => {
    expect(providerEditing(9, [one], all)).toBeNull();
  });
});
