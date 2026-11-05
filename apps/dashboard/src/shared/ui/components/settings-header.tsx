import { cn } from '@rustrak/ui';
import type { ReactNode } from 'react';

/**
 * A settings page's title and what it is for.
 *
 * The same reading as `PageHeader` -- title, one quiet line under it, a rule
 * -- but it lives inside the page's scroller and sticks to its top, bleeding
 * over the scroller's padding, because a settings page is a route's child and
 * cannot sit outside the layout's `Page` the way a top-level page does.
 *
 * The negative `top` is not a typo: a sticky box sticks inside its scroller's
 * padding, so `top-0` would hold it a gutter below the sheet's edge.
 *
 * With `tabs`, the tab strip's rule is the header's bottom edge: one line,
 * not a rule with tabs hanging under it.
 */
export function SettingsHeader({
  title,
  description,
  actions,
  tabs,
  leading,
}: {
  title: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
  /** A `TabList`, drawn as the header's bottom edge. */
  tabs?: ReactNode;
  /** Beside the title and its line: who or what the page is about. */
  leading?: ReactNode;
}) {
  return (
    <header
      className={cn(
        'settings-header sticky -top-page-gutter z-20 -mx-page-gutter -mt-page-gutter mb-2 bg-panel px-page-gutter pt-7',
        tabs ? 'pb-0' : 'border-b border-border-divider pb-6',
      )}
    >
      <div className="flex items-start justify-between gap-6">
        <div className="flex min-w-0 items-center gap-3.5">
          {leading}
          <div className="min-w-0">
            <h1 className="truncate text-page-title text-fg">{title}</h1>
            {description ? (
              <p className="mt-1.5 max-w-2xl text-value text-fg-subtle">
                {description}
              </p>
            ) : null}
          </div>
        </div>
        {actions ? (
          <div className="flex shrink-0 items-center gap-2">{actions}</div>
        ) : null}
      </div>
      {tabs ? (
        <div className="mt-5 [&_[data-slot=tab-frame]]:-mx-page-gutter [&_[data-slot=tab-frame]]:px-page-gutter [&_[data-slot=tab-frame]]:before:bg-border-divider">
          {tabs}
        </div>
      ) : null}
    </header>
  );
}
