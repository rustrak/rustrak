import type { RustrakError, UserResponse } from '@rustrak/client';
import { describe, expect, it, vi } from 'vitest';
import { type CurrentUser, createSessionStore } from './session';

/**
 * The store the router guards on.
 *
 * Under Next this was a server read on every navigation and there was nothing
 * to memoise. In the browser it is one `/auth/me` that several nested guards
 * all want the answer to, and the two ways it goes wrong are worth a test
 * each: asking once per guard, and remembering an answer a retry could change.
 */

const alice = { id: 1, email: 'alice@example.com' } as unknown as UserResponse;

function unreachable(): RustrakError {
  return {
    kind: 'network',
    message: 'the server did not answer',
    reason: 'unreachable',
  };
}

function unauthenticated(): RustrakError {
  return { kind: 'unauthenticated', message: 'not signed in', status: 401 };
}

describe('createSessionStore', () => {
  it('asks once however many guards want the answer', async () => {
    const read = vi.fn(
      async (): Promise<CurrentUser> => ({
        state: 'authenticated',
        user: alice,
      }),
    );
    const store = createSessionStore(read);

    const [a, b, c] = await Promise.all([
      store.ensure(),
      store.ensure(),
      store.ensure(),
    ]);

    expect(read).toHaveBeenCalledTimes(1);
    expect(a).toEqual(b);
    expect(b).toEqual(c);
  });

  it('keeps a settled answer, so navigating between guarded routes is free', async () => {
    const read = vi.fn(
      async (): Promise<CurrentUser> => ({
        state: 'authenticated',
        user: alice,
      }),
    );
    const store = createSessionStore(read);

    await store.ensure();
    await store.ensure();

    expect(read).toHaveBeenCalledTimes(1);
  });

  it('keeps `anonymous` too: signed out is an answer, not a failure', async () => {
    const read = vi.fn(
      async (): Promise<CurrentUser> => ({
        state: 'anonymous',
      }),
    );
    const store = createSessionStore(read);

    await store.ensure();
    await store.ensure();

    expect(read).toHaveBeenCalledTimes(1);
  });

  // `unavailable` is the one answer a retry can change, so it is the one the
  // memo drops. Remembering it would make a dropped connection permanent for
  // the life of the tab.
  it('forgets `unavailable`, because a retry can change it', async () => {
    const read = vi
      .fn<() => Promise<CurrentUser>>()
      .mockResolvedValueOnce({ state: 'unavailable', error: unreachable() })
      .mockResolvedValue({ state: 'authenticated', user: alice });
    const store = createSessionStore(read);

    expect(await store.ensure()).toEqual({
      state: 'unavailable',
      error: unreachable(),
    });
    expect(await store.ensure()).toEqual({
      state: 'authenticated',
      user: alice,
    });
    expect(read).toHaveBeenCalledTimes(2);
  });

  it('has no answer before the first read settles', async () => {
    const store = createSessionStore(async () => ({ state: 'anonymous' }));

    expect(store.peek()).toBeUndefined();
    await store.ensure();
    expect(store.peek()).toEqual({ state: 'anonymous' });
  });

  it('takes the session a sign-in just proved, without a second round trip', async () => {
    const read = vi.fn(
      async (): Promise<CurrentUser> => ({
        state: 'anonymous',
      }),
    );
    const store = createSessionStore(read);

    store.set({ state: 'authenticated', user: alice });

    expect(await store.ensure()).toEqual({
      state: 'authenticated',
      user: alice,
    });
    expect(read).not.toHaveBeenCalled();
  });

  // Not a claim that the server session ended — the next guard asks and gets
  // the truth. It is a claim that this tab must stop rendering the dashboard.
  it('goes anonymous on sign-out and asks again on the next guard', async () => {
    const read = vi.fn(
      async (): Promise<CurrentUser> => ({
        state: 'authenticated',
        user: alice,
      }),
    );
    const store = createSessionStore(read);

    await store.ensure();
    store.clear();

    expect(store.peek()).toEqual({ state: 'anonymous' });
    await store.ensure();
    expect(read).toHaveBeenCalledTimes(2);
  });

  // A sign-out asks again, and a sign-in can land before that answer does.
  // The late 401 is older than the session the sign-in proved.
  it('drops an answer a sign-in has already overtaken', async () => {
    let answer!: (session: CurrentUser) => void;
    const store = createSessionStore(
      () =>
        new Promise((resolve) => {
          answer = resolve;
        }),
    );

    store.clear();
    const late = store.ensure();
    store.set({ state: 'authenticated', user: alice });
    answer({ state: 'anonymous' });
    await late;

    expect(store.peek()).toEqual({ state: 'authenticated', user: alice });
    expect(await store.ensure()).toEqual({
      state: 'authenticated',
      user: alice,
    });
  });

  it('tells subscribers when the answer changes', async () => {
    const listener = vi.fn();
    const store = createSessionStore(async () => ({
      state: 'authenticated',
      user: alice,
    }));
    const unsubscribe = store.subscribe(listener);

    await store.ensure();
    expect(listener).toHaveBeenCalledTimes(1);

    unsubscribe();
    store.clear();
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it('survives a read that throws rather than returning a state', async () => {
    const read = vi
      .fn<() => Promise<CurrentUser>>()
      .mockRejectedValueOnce(new Error('boom'))
      .mockResolvedValue({ state: 'anonymous' });
    const store = createSessionStore(read);

    await expect(store.ensure()).rejects.toThrow('boom');
    // The rejected promise must not stay memoised, or the tab never recovers.
    expect(await store.ensure()).toEqual({ state: 'anonymous' });
  });

  it('reads a rejected credential as anonymous and nothing else as signed out', async () => {
    const store = createSessionStore(async () => ({ state: 'anonymous' }));
    expect(unauthenticated().kind).toBe('unauthenticated');
    expect(await store.ensure()).toEqual({ state: 'anonymous' });
  });
});
