import { projectFiles } from 'archunit';
import { describe, expect, it } from 'vitest';
import { isTestFile, withoutComments } from './predicates';

/**
 * A navigation commits at once, and every page shows its own skeleton while
 * its reads are in flight.
 *
 * **What this replaced.** Loaders used to `await` what their page could not
 * paint without. The router then kept the *previous* page on screen for the
 * whole wait, with nothing saying a click had registered, and only one route
 * had a `pendingComponent` to show past the threshold. Measured on this
 * router and React (1.170 / 19.3): an awaiting loader leaves the old page up
 * until `pendingMs`; a loader that only starts its reads, with the component
 * reading them through `useSuspenseQuery`, mounts the route's
 * `pendingComponent` in the next frame, every time.
 *
 * So the rules, each of which a habit from the old shape reintroduces:
 *
 * 1. **A loader starts reads and returns nothing.** No `async`, no `await`, no
 *    `return`, no expression body (`=> queryClient.query(...)` returns the
 *    promise, and the router waits on what a loader returns).
 * 2. **A route whose component suspends brings its own `pendingComponent`.**
 *    Without one, the route is not wrapped in a boundary of its own and the
 *    suspension climbs to the nearest one above: a whole layout, or the
 *    root, goes blank instead of one page showing its skeleton.
 * 3. **No `head` and no loader data.** With nothing awaited there is no
 *    loader data to title a page with; the page renders its own `<title>`,
 *    which React 19 hoists and keeps current as the data changes.
 * 4. **No router pending state as a loading indicator.** A filter or a page
 *    change shows the previous results, dimmed, through `useDeferredValue`;
 *    the router is never pending for it, so reading its state shows nothing.
 * 5. **No `prefetchQuery` or `ensureQueryData`.** Deprecated in TanStack
 *    Query; `void queryClient.query(options).catch(noop)` is what replaced
 *    them.
 * 6. **`beforeLoad` reads no data.** It runs before the navigation commits
 *    and the router waits on it, so a read there freezes the page exactly as
 *    an awaiting loader did. The session guard reads the session store, which
 *    is not a query; a redirect that depends on data is a component that
 *    suspends and renders `<Navigate replace />`.
 */

const posix = (path: string) => path.split('\\').join('/');

/** A file the route generator reads: under `routes/`, outside `-` folders. */
const isRouteFile = (path: string) => {
  const [, below] = posix(path).split(/(?:^|\/)src\/routes\//);
  return (
    below !== undefined &&
    !below.split('/').some((segment) => segment.startsWith('-')) &&
    !isTestFile(path)
  );
};

/** The text between a bracket at `open` and the one that closes it. */
function balanced(source: string, open: number): string | null {
  const pairs: Record<string, string> = { '(': ')', '{': '}' };
  const closing = pairs[source[open] ?? ''];
  if (!closing) return null;
  let depth = 0;
  for (let index = open; index < source.length; index++) {
    if (source[index] === source[open]) depth++;
    else if (source[index] === closing && --depth === 0) {
      return source.slice(open + 1, index);
    }
  }
  return null;
}

/**
 * Why a route's loader would make the router wait, or `null` if it starts its
 * reads and returns.
 */
function blockingLoader(source: string): string | null {
  const match = /\bloader:\s*/.exec(source);
  if (!match) return null;
  let at = match.index + match[0].length;
  if (source.startsWith('async', at)) return 'is async';
  const params = balanced(source, at);
  if (params === null) return 'is not an arrow function';
  at += params.length + 2;
  const arrow = /^\s*=>\s*/.exec(source.slice(at));
  if (!arrow) return 'is not an arrow function';
  at += arrow[0].length;
  const body = balanced(source, at);
  if (source[at] !== '{' || body === null) return 'returns an expression';
  if (/\bawait\b/.test(body)) return 'awaits';
  if (/\breturn\b/.test(body)) return 'returns a value';
  return null;
}

/** Whether a route's `beforeLoad` reaches the query cache. */
function beforeLoadReads(source: string): boolean {
  const match = /\bbeforeLoad:\s*(?:async\s*)?/.exec(source);
  if (!match) return false;
  const at = match.index + match[0].length;
  const params = balanced(source, at) ?? '';
  const arrow = /^\s*=>\s*/.exec(source.slice(at + params.length + 2));
  const body = arrow
    ? (balanced(source, at + params.length + 2 + arrow[0].length) ?? '')
    : '';
  return /\bqueryClient\b/.test(params + body);
}

const SUSPENDS = /\buseSuspense(?:Query|Queries|InfiniteQuery)\b/;
const ROUTER_PENDING =
  /useRouterState\(\s*\{\s*select:[^}]*\b(?:isLoading|status)\b/;
const DEPRECATED =
  /\b(?:prefetchQuery|ensureQueryData|prefetchInfiniteQuery|ensureInfiniteQueryData)\b/;

describe('routes navigate at once and load behind their own skeleton', () => {
  it('reads the population it expects to read', async () => {
    const routes = await projectFiles()
      .inFolder('src/routes/**')
      .shouldNot()
      .adhereTo((file) => isRouteFile(file.path), 'counted')
      .check();

    expect(routes.length).toBeGreaterThanOrEqual(55);
  });

  it('recognises a loader that blocks', () => {
    const start = `loader: ({ context: { queryClient } }) => {
      void queryClient.query(a).catch(noop);
    },`;
    expect(blockingLoader(start)).toBeNull();
    expect(blockingLoader('loader: async ({ context }) => {}')).toBe(
      'is async',
    );
    expect(blockingLoader('loader: ({ context }) => context.q.query(a),')).toBe(
      'returns an expression',
    );
    expect(blockingLoader('loader: () => { return x; },')).toBe(
      'returns a value',
    );
    expect(blockingLoader('loader: () => { await x; },')).toBe('awaits');
  });

  it('has no loader that makes the router wait', async () => {
    const rule = projectFiles()
      .inFolder('src/routes/**')
      .shouldNot()
      .adhereTo(
        (file) =>
          isRouteFile(file.path) &&
          blockingLoader(withoutComments(file.content)) !== null,
        'has a loader the router waits on: it holds the previous page on screen with no feedback',
      );

    await expect(rule).toPassAsync();
  });

  it('reads no data in beforeLoad', async () => {
    expect(
      beforeLoadReads(
        'beforeLoad: async ({ context: { queryClient } }) => { await x; },',
      ),
    ).toBe(true);
    expect(
      beforeLoadReads('beforeLoad: async () => { await session.ensure(); },'),
    ).toBe(false);

    const rule = projectFiles()
      .inFolder('src/routes/**')
      .shouldNot()
      .adhereTo(
        (file) =>
          isRouteFile(file.path) &&
          beforeLoadReads(withoutComments(file.content)),
        'reads data in beforeLoad: the router waits on it before the navigation commits',
      );

    await expect(rule).toPassAsync();
  });

  it('gives every route that suspends its own pendingComponent', async () => {
    const rule = projectFiles()
      .inFolder('src/routes/**')
      .shouldNot()
      .adhereTo((file) => {
        const source = withoutComments(file.content);
        return (
          isRouteFile(file.path) &&
          SUSPENDS.test(source) &&
          !/\bpendingComponent:/.test(source)
        );
      }, 'suspends without a pendingComponent: the suspension blanks the nearest boundary above it');

    await expect(rule).toPassAsync();
  });

  it('titles pages from the component, not from loader data', async () => {
    const rule = projectFiles()
      .inFolder('src/**')
      .shouldNot()
      .adhereTo((file) => {
        const source = withoutComments(file.content);
        return (
          !isTestFile(file.path) &&
          /\bhead:\s*\(|\buseLoaderData\b|\bloaderData\b|\bHeadContent\b/.test(
            source,
          )
        );
      }, 'reads loader data or a route head: nothing is awaited, so there is none to read');

    await expect(rule).toPassAsync();
  });

  it('never shows loading through the router state', async () => {
    const rule = projectFiles()
      .inFolder('src/**')
      .shouldNot()
      .adhereTo(
        (file) =>
          !isTestFile(file.path) &&
          ROUTER_PENDING.test(withoutComments(file.content)),
        'reads the router pending state: loaders never wait, so it is never set for a page change',
      );

    await expect(rule).toPassAsync();
  });

  it('uses no deprecated prefetch method', async () => {
    const rule = projectFiles()
      .inFolder('src/**')
      .shouldNot()
      .adhereTo(
        (file) =>
          !isTestFile(file.path) &&
          DEPRECATED.test(withoutComments(file.content)),
        'calls a method TanStack Query deprecated: use `void queryClient.query(options).catch(noop)`',
      );

    await expect(rule).toPassAsync();
  });
});
