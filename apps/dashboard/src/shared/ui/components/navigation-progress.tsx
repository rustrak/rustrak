import { useRouterState } from '@tanstack/react-router';

/**
 * A bar across the top while the next screen's data loads.
 *
 * The router keeps the previous page on screen during a navigation, which is
 * right for a fast one and reads as a frozen page for a slow one. The delay
 * on the way in keeps a fast navigation from flashing it at all.
 */
export function NavigationProgress() {
  const pending = useRouterState({ select: (s) => s.status === 'pending' });

  if (!pending) return null;

  return (
    <div
      aria-hidden
      className="pointer-events-none fixed inset-x-0 top-0 z-[100] h-0.5 origin-left bg-primary animate-[nav-progress_2s_ease-out_150ms_both]"
    />
  );
}
