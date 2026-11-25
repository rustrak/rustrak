import type { ReactNode } from 'react';
import { tv } from '../../lib/tv';

/**
 * A settings page's title and what it is for.
 *
 * It lives inside the page's scroller and sticks to its top, bleeding over the
 * scroller's padding: a settings page is a route's child and cannot sit
 * outside the layout the way a top-level page does. The negative `top` is not
 * a typo: a sticky box sticks inside its scroller's padding, so `top-0` would
 * hold it a gutter below the sheet's edge.
 *
 * With `tabs`, the tab strip's rule is the header's bottom edge: one line,
 * not a rule with tabs hanging under it.
 */
const header = tv({
  slots: {
    root: 'settings-header sticky -top-page-gutter z-20 -mx-page-gutter -mt-page-gutter mb-2 bg-panel px-page-gutter pt-7',
    // Wraps instead of squeezing: on a phone the actions drop under the title
    // rather than crushing it to an ellipsis.
    row: 'flex flex-wrap items-start justify-between gap-x-6 gap-y-4',
    identity: 'flex min-w-0 max-w-full items-center gap-3.5',
    title: 'truncate text-page-title text-fg',
    description: 'mt-1.5 max-w-2xl text-value text-fg-subtle',
    actions: 'flex max-w-full flex-wrap items-center gap-2',
    tabs: 'mt-5 [&_[data-slot=tab-frame]]:-mx-page-gutter [&_[data-slot=tab-frame]]:px-page-gutter [&_[data-slot=tab-frame]]:before:bg-border-divider',
  },
  variants: {
    tabbed: {
      true: { root: 'pb-0' },
      false: { root: 'border-b border-border-divider pb-6' },
    },
  },
});

export interface SettingsHeaderProps {
  title: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
  /** A `TabList`, drawn as the header's bottom edge. */
  tabs?: ReactNode;
  /** Beside the title and its line: who or what the page is about. */
  leading?: ReactNode;
}

export function SettingsHeader({
  title,
  description,
  actions,
  tabs,
  leading,
}: SettingsHeaderProps) {
  const s = header({ tabbed: Boolean(tabs) });
  return (
    <header className={s.root()}>
      <div className={s.row()}>
        <div className={s.identity()}>
          {leading}
          <div className="min-w-0">
            <h1 className={s.title()}>{title}</h1>
            {description ? (
              <p className={s.description()}>{description}</p>
            ) : null}
          </div>
        </div>
        {actions ? <div className={s.actions()}>{actions}</div> : null}
      </div>
      {tabs ? <div className={s.tabs()}>{tabs}</div> : null}
    </header>
  );
}

SettingsHeader.displayName = 'SettingsHeader';
