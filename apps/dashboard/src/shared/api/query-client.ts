import { QueryClient } from '@tanstack/react-query';
import type { SessionStore } from './session';

/**
 * The one cache every read goes through.
 *
 * `@rustrak/client` returns a `Result` and never throws for an expected
 * failure, so a failed request is cached data like any other and the screen
 * renders its failure branch. A rejection is a bug, and goes to the nearest
 * error boundary instead of retrying.
 *
 * `staleTime` is what stops a loader's fetch from being repeated by the
 * component that mounts right after it. Past it, going back to a screen shows
 * what it had at once and refreshes underneath.
 */
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 10_000,
      retry: false,
      throwOnError: true,
    },
  },
});

/**
 * Query key roots.
 *
 * Everything that belongs to one project sits under `project(id)`, so a write
 * to that project invalidates exactly its reads, whichever slice they are in.
 * The instance-wide lists have a root each.
 */
export const scope = {
  project: (id: number) => ['project', id] as const,
  projects: ['projects'] as const,
  tokens: ['tokens'] as const,
  integrations: ['integrations'] as const,
  team: ['team'] as const,
  storage: ['storage'] as const,
  invitation: ['invitation'] as const,
};

/** After a write to one of the instance-wide lists. */
export function invalidate(key: readonly unknown[]): Promise<void> {
  return queryClient.invalidateQueries({ queryKey: key });
}

/**
 * After a write whose reach is the whole instance: a cleanup that deletes
 * events changes every count on every screen.
 */
export function invalidateAll(): Promise<void> {
  return queryClient.invalidateQueries();
}

/**
 * After a write to one project: its own reads, and the instance-wide project
 * list whose rows carry its name and counts.
 */
export function invalidateProject(id: number): Promise<void> {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: scope.project(id) }),
    queryClient.invalidateQueries({ queryKey: scope.projects }),
  ]).then(() => undefined);
}

/**
 * Forget every answer when the reader changes: signing out, or someone else
 * signing in on the same tab, must not show the next person the last one's
 * data while it refetches.
 */
export function bindCacheToSession(
  cache: QueryClient,
  store: SessionStore,
): () => void {
  const readerOf = () => {
    const answer = store.peek();
    return answer?.state === 'authenticated' ? answer.user.id : null;
  };
  let reader = readerOf();

  return store.subscribe(() => {
    const next = readerOf();
    if (next !== reader) cache.clear();
    reader = next;
  });
}
