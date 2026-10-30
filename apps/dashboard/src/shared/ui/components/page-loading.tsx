import type { ReactNode } from 'react';
import { useTranslations } from 'use-intl';

/**
 * What a route's `pendingComponent` wraps its skeleton in.
 *
 * The bars inside are `aria-hidden`; the status beside them is what tells a
 * screen reader the page is loading, once. It holds only that word: a live
 * region announces its contents, and the skeleton's real headings inside one
 * would be read out with it. No box of its own either, so the skeleton lays
 * out exactly as the page it stands for: a settings page's header still
 * sticks to the layout's scroller, a list page still fills the sheet.
 */
export function PageLoading({ children }: { children: ReactNode }) {
  const t = useTranslations('common');
  return (
    <>
      <span role="status" className="sr-only">
        {t('loading')}
      </span>
      {children}
    </>
  );
}
