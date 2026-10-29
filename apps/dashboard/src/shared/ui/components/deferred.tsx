import type { Result, RustrakError } from '@rustrak/client';
import { type ReactNode, Suspense, use } from 'react';
import { LoadFailure } from '@/shared/ui/components/load-failure';

/**
 * One panel's read, returned unawaited by its route's loader: the page paints
 * without it, the panel holds a placeholder until it lands, and a failure
 * stays in the panel. A loader awaiting every panel made the page as slow as
 * the slowest one, and one failure took the whole page down.
 *
 * React's `use` rather than the router's `<Await>`: the router stays in
 * `routes/`, and the promise is the loader's, so it is the same one on every
 * render.
 */
export function Deferred<T>({
  promise,
  height,
  title,
  fallback,
  children,
}: {
  promise: Promise<Result<T, RustrakError>>;
  /** Of the placeholder, when there is no `fallback`. */
  height?: number;
  /** Which read failed, when it does. */
  title: string;
  fallback?: ReactNode;
  children: (data: T) => ReactNode;
}) {
  return (
    <Suspense
      fallback={
        fallback ?? (
          <div
            className="w-full animate-pulse rounded-lg bg-surface-hover motion-reduce:animate-none"
            style={{ height }}
          />
        )
      }
    >
      <Resolved promise={promise} title={title}>
        {children}
      </Resolved>
    </Suspense>
  );
}

function Resolved<T>({
  promise,
  title,
  children,
}: {
  promise: Promise<Result<T, RustrakError>>;
  title: string;
  children: (data: T) => ReactNode;
}) {
  const result = use(promise);
  if (!result.success) {
    return (
      <LoadFailure
        error={result.error}
        title={title}
        notFoundOnMissing={false}
      />
    );
  }
  return children(result.data);
}
