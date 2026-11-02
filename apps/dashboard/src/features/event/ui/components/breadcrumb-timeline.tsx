import {
  ClickIcon,
  ConsoleIcon,
  DotIcon,
  type IconComponent,
  InfoIcon,
  NavigationIcon,
  NetworkIcon,
  StorageIcon,
} from '@rustrak/ui';
import { useFormatter, useTranslations } from 'use-intl';
import type { GroupedBreadcrumb } from '@/features/event/lib/breadcrumbs';
import { cn } from '@/shared/lib/utils';

/** Pick a timeline icon from the crumb category/type. */
function crumbIcon(crumb: {
  category?: string;
  type?: string;
  level?: string;
}): IconComponent {
  const c = `${crumb.category ?? ''} ${crumb.type ?? ''}`.toLowerCase();
  if (c.includes('navigation')) return NavigationIcon;
  if (/http|xhr|fetch|request/.test(c)) return NetworkIcon;
  if (/console|debug/.test(c)) return ConsoleIcon;
  if (/ui|click|touch/.test(c)) return ClickIcon;
  if (/query|sql|\bdb\b/.test(c)) return StorageIcon;
  if (crumb.level === 'error' || /error|exception/.test(c)) return InfoIcon;
  if (c.includes('info')) return InfoIcon;
  return DotIcon;
}

function levelColor(level?: string): string {
  switch (level) {
    case 'fatal':
    case 'error':
      return 'text-sev-error-fg';
    case 'warning':
      return 'text-sev-warning-fg';
    default:
      return 'text-fg-subtle';
  }
}

/**
 * What happened before the error, one line per crumb: its kind, what it
 * says, when. The data a crumb carries stays folded under its line -- the
 * trail is read as a sequence first, and only one or two crumbs are ever
 * opened.
 */
export function BreadcrumbTimeline({
  grouped,
}: {
  grouped: GroupedBreadcrumb[];
}) {
  const format = useFormatter();
  const t = useTranslations('events');

  return (
    <ol className="divide-y divide-border-divider overflow-hidden rounded-lg border border-border-subtle">
      {grouped.map(({ crumb, count }, i) => {
        const Icon = crumbIcon(crumb);
        const hasData = !!crumb.data && Object.keys(crumb.data).length > 0;
        const line = (
          <>
            <Icon
              aria-hidden="true"
              className={cn('size-3.5 shrink-0', levelColor(crumb.level))}
            />
            <span className="w-28 shrink-0 truncate font-mono text-mono-sm text-fg-meta">
              {crumb.category || crumb.type || t('breadcrumbs.defaultCategory')}
              {count > 1 ? (
                <span className="ms-1 text-fg-ghost">×{count}</span>
              ) : null}
            </span>
            {/* Wraps rather than truncates: a log line or a request is often
                the whole point of the crumb, and a row without data has no
                expanded state to show the rest in. */}
            <span className="min-w-0 flex-1 break-words text-value text-fg-secondary">
              {crumb.message ?? ''}
            </span>
            {crumb.timestamp ? (
              <time className="shrink-0 font-mono text-mono-sm text-fg-ghost tabular-nums">
                {format.dateTime(new Date(crumb.timestamp * 1000), 'time')}
              </time>
            ) : null}
          </>
        );

        return (
          // `BreadcrumbTimeline` is presentational: no state, no filtering, no
          // sort. `grouped` is built once from the event's crumbs, and two
          // crumbs can be identical in every field but their position, so the
          // index is the only thing that tells them apart.
          // react-doctor-disable-next-line react-doctor/no-array-index-as-key
          <li key={i} className="bg-surface">
            {hasData ? (
              <details className="group/crumb">
                <summary className="flex cursor-pointer list-none items-center gap-3 px-3 py-2 hover:bg-surface-hover [&::-webkit-details-marker]:hidden">
                  {line}
                </summary>
                <pre className="overflow-x-auto border-border-divider border-t bg-panel px-3 py-2 font-mono text-mono-sm text-fg-subtle">
                  {JSON.stringify(crumb.data, null, 2)}
                </pre>
              </details>
            ) : (
              <div className="flex items-center gap-3 px-3 py-2">{line}</div>
            )}
          </li>
        );
      })}
    </ol>
  );
}
