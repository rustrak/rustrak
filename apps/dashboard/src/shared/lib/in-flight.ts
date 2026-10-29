const inFlight = new Map<string, Promise<unknown>>();

/**
 * One request for everyone asking for `key` while it is in flight, and a new
 * one once it has answered: nothing is kept, so nothing goes stale.
 *
 * A project page's layout and the page under it both read the project, and
 * the router runs their loaders at the same time; without this every
 * navigation inside a project asked the server for it twice.
 */
export function shareInFlight<T>(
  key: string,
  run: () => Promise<T>,
): Promise<T> {
  const pending = inFlight.get(key);
  if (pending) return pending as Promise<T>;
  const started = run().finally(() => inFlight.delete(key));
  inFlight.set(key, started);
  return started;
}
