import { cn } from '@rustrak/ui';
import type { ReactNode } from 'react';

/*
 * The layout every detail page shares (a monitor, a release, an alert rule,
 * an endpoint, an issue): a header across the top, then the reading column
 * with its rail beside it on a wide screen and under it on a narrow one.
 */

/** What the page is on the left, what can be done to it on the right. */
export function DetailHeader({ children }: { children: ReactNode }) {
  return (
    <header className="@container shrink-0 border-border-divider border-b px-page-gutter pt-6 pb-5">
      <div className="flex flex-col gap-4 @4xl:flex-row @4xl:items-start @4xl:justify-between @4xl:gap-8">
        {children}
      </div>
    </header>
  );
}

/** The reading column and the rail under the header. */
export function DetailBody({
  busy,
  children,
}: {
  /** Another scope is on its way: what is on screen stays, dimmed. */
  busy?: boolean;
  children: ReactNode;
}) {
  return (
    <div
      aria-busy={busy}
      className={cn(
        'flex min-h-0 flex-1 max-lg:flex-col max-lg:overflow-y-auto',
        busy && 'opacity-60',
      )}
    >
      {children}
    </div>
  );
}

export function DetailMain({ children }: { children: ReactNode }) {
  return (
    <main className="min-w-0 flex-1 lg:overflow-y-auto">
      <div className="flex flex-col gap-5 px-page-gutter pt-5 pb-8">
        {children}
      </div>
    </main>
  );
}

export function DetailRail({ children }: { children: ReactNode }) {
  return (
    <aside className="border-border-divider bg-panel max-lg:border-t lg:w-80 lg:shrink-0 lg:overflow-y-auto lg:border-l xl:w-88">
      {children}
    </aside>
  );
}
