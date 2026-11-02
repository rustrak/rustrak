import type { RustrakError, UserResponse } from '@rustrak/client';
import { Err, Ok } from '@rustrak/client';
import { notifyManager, QueryObserver } from '@tanstack/react-query';
import { createFormatter, type Messages } from 'use-intl';
import { describe, expect, it, vi } from 'vitest';
import { createIntlStore } from '@/shared/i18n/intl';
import {
  ApiError,
  clearOnUserChange,
  createQueryClient,
  orThrow,
  refreshNowOnData,
} from './query';
import { createSessionStore } from './session';

const down: RustrakError = {
  kind: 'network',
  reason: 'timeout',
  message: 'The request timed out.',
};

describe('orThrow', () => {
  it('answers the data of a read that succeeded', async () => {
    expect(await orThrow(Promise.resolve(Ok([1, 2])))).toEqual([1, 2]);
  });

  it('throws the failure as an ApiError that keeps it whole', async () => {
    const thrown = await orThrow(Promise.resolve(Err(down))).catch((e) => e);

    expect(thrown).toBeInstanceOf(ApiError);
    expect(thrown.error).toBe(down);
    expect(thrown.message).toBe('The request timed out.');
  });
});

describe('createQueryClient', () => {
  it('leaves retrying to the API client, which already retries what can succeed', () => {
    const client = createQueryClient();

    expect(client.getDefaultOptions().queries?.retry).toBe(false);
  });

  it('marks stale what a mutation says it changed, once it settles', async () => {
    const client = createQueryClient();
    await client.query({
      queryKey: ['projects', 1, 'issues'],
      queryFn: () => 1,
    });

    await client
      .getMutationCache()
      .build(client, {
        mutationFn: async () => 'done',
        meta: { invalidates: [['projects', 1]] },
      })
      .execute(undefined);

    expect(client.getQueryState(['projects', 1, 'issues'])?.isInvalidated).toBe(
      true,
    );
  });

  it('leaves alone what the mutation did not name', async () => {
    const client = createQueryClient();
    await client.query({
      queryKey: ['projects', 2, 'issues'],
      queryFn: () => 1,
    });

    await client
      .getMutationCache()
      .build(client, {
        mutationFn: async () => 'done',
        meta: { invalidates: [['projects', 1]] },
      })
      .execute(undefined);

    expect(client.getQueryState(['projects', 2, 'issues'])?.isInvalidated).toBe(
      false,
    );
  });
});

describe('clearOnUserChange', () => {
  const user = (id: number) =>
    ({ state: 'authenticated', user: { id } as UserResponse }) as const;

  async function cached() {
    const client = createQueryClient();
    await client.query({ queryKey: ['team'], queryFn: () => ['alice'] });
    return client;
  }

  it('drops what one reader cached when another signs in', async () => {
    const client = await cached();
    const store = createSessionStore(async () => user(1));
    await store.ensure();
    clearOnUserChange(client, store);

    store.set(user(2));

    expect(client.getQueryData(['team'])).toBeUndefined();
  });

  it('drops it on sign-out', async () => {
    const client = await cached();
    const store = createSessionStore(async () => user(1));
    await store.ensure();
    clearOnUserChange(client, store);

    store.clear();

    expect(client.getQueryData(['team'])).toBeUndefined();
  });

  it('keeps it when the same reader is confirmed again', async () => {
    const client = await cached();
    const store = createSessionStore(async () => user(1));
    await store.ensure();
    clearOnUserChange(client, store);

    store.set(user(1));

    expect(client.getQueryData(['team'])).toEqual(['alice']);
  });
});

describe('refreshNowOnData', () => {
  it('moves the clock when a read succeeds', async () => {
    const client = createQueryClient();
    const clock = { refreshNow: vi.fn() };
    refreshNowOnData(client, clock);

    await client.query({ queryKey: ['team'], queryFn: () => ['alice'] });

    expect(clock.refreshNow).toHaveBeenCalledTimes(1);
  });

  it('leaves it when a read fails', async () => {
    const client = createQueryClient();
    const clock = { refreshNow: vi.fn() };
    refreshNowOnData(client, clock);

    await client
      .query({
        queryKey: ['team'],
        queryFn: () => orThrow(Promise.resolve(Err(down))),
      })
      .catch(() => undefined);

    expect(clock.refreshNow).not.toHaveBeenCalled();
  });

  it('moves it once for updates that land in the same tick', async () => {
    const client = createQueryClient();
    const clock = { refreshNow: vi.fn() };
    refreshNowOnData(client, clock);

    client.setQueryData(['a'], 1);
    client.setQueryData(['b'], 2);
    await Promise.resolve();

    expect(clock.refreshNow).toHaveBeenCalledTimes(1);
  });

  it('moves it again for an update in a later tick', async () => {
    const client = createQueryClient();
    const clock = { refreshNow: vi.fn() };
    refreshNowOnData(client, clock);

    client.setQueryData(['a'], 1);
    await Promise.resolve();
    client.setQueryData(['b'], 2);
    await Promise.resolve();

    expect(clock.refreshNow).toHaveBeenCalledTimes(2);
  });

  // `useSuspenseQuery` throws this promise, and React renders again once it
  // resolves. The clock must already have moved by then.
  it('moves it before a suspended page renders again', async () => {
    const client = createQueryClient();
    const clock = { refreshNow: vi.fn() };
    refreshNowOnData(client, clock);
    const options = { queryKey: ['monitor'], queryFn: () => 'data' };

    await new QueryObserver(client, options).fetchOptimistic(options);

    expect(clock.refreshNow).toHaveBeenCalledTimes(1);
  });

  // The whole path but React: the page must never see the new data against
  // the old clock, or a check-in from a minute ago reads "in 33 minutes".
  it('moves it before the new data reaches the page', async () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    let unsubscribe = () => {};
    try {
      vi.setSystemTime(new Date('2026-01-01T06:00:00Z'));
      const intl = createIntlStore({
        readSession: async () => ({ state: 'anonymous' }),
        browserLanguages: () => ['en'],
        loadMessages: async () => ({}) as Messages,
      });
      await intl.ensure();
      const client = createQueryClient();
      refreshNowOnData(client, intl);

      vi.setSystemTime(new Date('2026-01-01T06:34:00Z'));
      const observer = new QueryObserver(client, {
        queryKey: ['monitor'],
        queryFn: () => '2026-01-01T06:33:00Z',
      });
      const seen = new Promise<string>((resolve) => {
        unsubscribe = observer.subscribe(
          // Wrapped as `useQuery` wraps its listener.
          notifyManager.batchCalls((result) => {
            if (!result.data) return;
            const { now } = intl.snapshot();
            const format = createFormatter({ locale: 'en', now });
            resolve(format.relativeTime(new Date(result.data)));
          }),
        );
      });

      expect(await seen).toBe('1 minute ago');
    } finally {
      unsubscribe();
      vi.useRealTimers();
    }
  });
});
