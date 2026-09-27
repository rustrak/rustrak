import type { User } from '@rustrak/client';
import { QueryClient } from '@tanstack/react-query';
import { describe, expect, it } from 'vitest';
import { bindCacheToSession } from './query-client';
import { type CurrentUser, createSessionStore } from './session';

function signedIn(id: number): CurrentUser {
  return { state: 'authenticated', user: { id } as User };
}

function setup() {
  const cache = new QueryClient();
  const session = createSessionStore(async () => ({ state: 'anonymous' }));
  bindCacheToSession(cache, session);
  cache.setQueryData(['projects'], 'the previous reader’s projects');
  return { cache, session };
}

// One reader's answers must never be shown to the next one on the same tab.
describe('bindCacheToSession', () => {
  it('drops the cache on sign-out', () => {
    const { cache, session } = setup();
    session.set(signedIn(1));
    cache.setQueryData(['projects'], 'alice’s projects');

    session.clear();

    expect(cache.getQueryData(['projects'])).toBeUndefined();
  });

  it('drops the cache when someone else signs in', () => {
    const { cache, session } = setup();
    session.set(signedIn(1));
    cache.setQueryData(['projects'], 'alice’s projects');

    session.set(signedIn(2));

    expect(cache.getQueryData(['projects'])).toBeUndefined();
  });

  it('keeps it when the same reader’s row is republished', () => {
    const { cache, session } = setup();
    session.set(signedIn(1));
    cache.setQueryData(['projects'], 'alice’s projects');

    session.set(signedIn(1));

    expect(cache.getQueryData(['projects'])).toBe('alice’s projects');
  });
});
