import { useTranslations } from 'use-intl';
import {
  type Breadcrumb,
  getSummaryBreadcrumbs,
} from '@/features/event/lib/breadcrumbs';
import { BreadcrumbsExpand } from './breadcrumbs-expand';

/** `items` as `normalizeBreadcrumbs` (event-schema) returns them. */
export function Breadcrumbs({
  breadcrumbs: items,
}: {
  breadcrumbs: Breadcrumb[];
}) {
  const t = useTranslations('events');

  if (items.length === 0) {
    return (
      <div className="py-8 text-center text-meta text-fg-subtle">
        {t('breadcrumbs.empty')}
      </div>
    );
  }

  const summaryItems = getSummaryBreadcrumbs(items);

  return <BreadcrumbsExpand items={items} summaryItems={summaryItems} />;
}
