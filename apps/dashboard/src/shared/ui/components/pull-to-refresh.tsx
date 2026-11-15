import { cn, RefreshIcon } from '@rustrak/ui';
import { useQueryClient } from '@tanstack/react-query';
import { type ComponentProps, useEffect, useRef, useState } from 'react';

/** How far the finger travels, after resistance, before a release refreshes. */
const THRESHOLD = 64;
/** Pulling moves the indicator at this fraction of the finger: it resists. */
const RESISTANCE = 0.5;

/**
 * The content area's scroller, with pull-to-refresh on a touch screen.
 *
 * An error tracker on a phone is mostly opened to look again: has it stopped,
 * is it still coming in. The gesture every app has taught for that is pulling
 * the list down, and the browser's own version is not available here: the
 * page itself never scrolls (`overscroll-behavior: none` in the design
 * system), only this box does, and in the installed app there is no browser
 * chrome to reload from at all.
 *
 * A release past the threshold refetches the queries on screen, not the page:
 * the data comes back in place, with the scroll position and anything typed
 * into a filter untouched. Only from the very top, so it never fights a
 * scroll back up a list.
 */
export function PullToRefresh({
  className,
  children,
  ...props
}: ComponentProps<'div'>) {
  const queryClient = useQueryClient();
  const scroller = useRef<HTMLDivElement>(null);
  const [pull, setPull] = useState(0);
  const [refreshing, setRefreshing] = useState(false);

  useEffect(() => {
    const node = scroller.current;
    if (!node) return;
    let start: number | null = null;
    let distance = 0;

    const onStart = (event: TouchEvent) => {
      start =
        node.scrollTop <= 0 && event.touches.length === 1
          ? (event.touches[0]?.clientY ?? null)
          : null;
    };
    const onMove = (event: TouchEvent) => {
      if (start === null) return;
      const delta = (event.touches[0]?.clientY ?? start) - start;
      if (delta <= 0 || node.scrollTop > 0) {
        distance = 0;
        setPull(0);
        return;
      }
      // Ours from here on: the box is at its top and the finger goes down.
      event.preventDefault();
      distance = Math.min(delta * RESISTANCE, THRESHOLD * 1.5);
      setPull(distance);
    };
    const onEnd = () => {
      if (start === null) return;
      start = null;
      if (distance >= THRESHOLD) {
        setRefreshing(true);
        void queryClient
          .refetchQueries({ type: 'active' })
          .finally(() => setRefreshing(false));
      }
      distance = 0;
      setPull(0);
    };

    // `passive: false`, or `preventDefault` above is ignored and the box
    // rubber-bands under the indicator. React's touch listeners are passive.
    node.addEventListener('touchstart', onStart, { passive: true });
    node.addEventListener('touchmove', onMove, { passive: false });
    node.addEventListener('touchend', onEnd);
    node.addEventListener('touchcancel', onEnd);
    return () => {
      node.removeEventListener('touchstart', onStart);
      node.removeEventListener('touchmove', onMove);
      node.removeEventListener('touchend', onEnd);
      node.removeEventListener('touchcancel', onEnd);
    };
  }, [queryClient]);

  const shown = refreshing ? THRESHOLD : pull;
  const armed = pull >= THRESHOLD;

  return (
    <div ref={scroller} className={cn('relative', className)} {...props}>
      <div
        aria-hidden="true"
        className={cn(
          'pointer-events-none sticky top-0 z-30 flex h-0 justify-center',
          shown === 0 && 'invisible',
        )}
      >
        <span
          className={cn(
            'mt-2 flex size-9 items-center justify-center rounded-pill',
            'border border-border-subtle bg-surface shadow-raised',
            armed || refreshing ? 'text-fg-brand' : 'text-fg-subtle',
            // Follows the finger with no lag; eases back when let go.
            pull === 0 &&
              'transition-[translate,opacity] duration-moderate ease-standard',
          )}
          style={{
            translate: `0 ${shown - 44}px`,
            opacity: Math.min(shown / THRESHOLD, 1),
          }}
        >
          <RefreshIcon
            size="2xl"
            className={refreshing ? 'animate-spin' : undefined}
            style={refreshing ? undefined : { rotate: `${pull * 4}deg` }}
          />
        </span>
      </div>
      {children}
    </div>
  );
}
