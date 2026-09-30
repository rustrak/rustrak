import {
  useNavigate,
  useRouter as useTanStackRouter,
} from '@tanstack/react-router';
import { useMemo } from 'react';
import { projectHrefWithEnvironment } from '@/shared/lib/project-environment';

/**
 * The four things this application ever asked a router to do.
 *
 * It reads like `next/navigation`'s `useRouter` because it is the same four
 * calls at the same forty call sites; only what happens underneath changed.
 * Keeping the shape meant the framework move touched the import line of those
 * files and nothing else, which is the difference between a diff a reviewer
 * can read and one they can only trust.
 *
 * `refresh` is the one worth reading twice. Under Next it re-ran the Server
 * Components for the current URL and reconciled, so scroll position and open
 * dialogs survived. `router.invalidate()` is the same promise kept the same
 * way: it re-runs the route loaders and re-renders from their answers, without
 * unmounting the tree. `location.reload()` would have been neither.
 */
export interface AppRouter {
  push(href: string): void;
  replace(href: string): void;
  back(): void;
  /** Re-run the loaders for the routes currently on screen. */
  refresh(): Promise<void>;
}

export function useRouter(): AppRouter {
  const navigate = useNavigate();
  const router = useTanStackRouter();

  return useMemo(
    () => ({
      push: (href: string) => {
        void navigate({
          href: projectHrefWithEnvironment(href, window.location.href),
        });
      },
      replace: (href: string) => {
        void navigate({
          href: projectHrefWithEnvironment(href, window.location.href),
          replace: true,
        });
      },
      back: () => {
        router.history.back();
      },
      refresh: () => router.invalidate(),
    }),
    [navigate, router],
  );
}
