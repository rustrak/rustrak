import { Button } from '@rustrak/ui';
import { useSuspenseInfiniteQuery } from '@tanstack/react-query';
import { useFormatter, useTranslations } from 'use-intl';
import { issueQueries } from '@/features/issue/api/queries';

/**
 * The feedback users sent about an error issue, newest first: the first page
 * from the route's loader, older pages on request.
 */
export function UserFeedback({
  projectId,
  issueId,
}: {
  projectId: number;
  issueId: string;
}) {
  const t = useTranslations('projectPages.event.feedback');
  const format = useFormatter();
  const {
    data: { pages },
    hasNextPage,
    fetchNextPage,
    isFetchingNextPage,
    isFetchNextPageError,
  } = useSuspenseInfiniteQuery(issueQueries.userReports(projectId, issueId));
  const last = pages[pages.length - 1]!;
  // A report sent while paging shifts the offsets; keep each one once.
  const reports = [
    ...new Map(pages.flatMap((p) => p.items).map((r) => [r.id, r])).values(),
  ];

  return (
    <div className="space-y-3">
      <ul className="divide-y divide-border-divider">
        {reports.map((report) => (
          <li key={report.id} className="py-3 first:pt-0 last:pb-0">
            <div className="flex items-baseline gap-2 text-body">
              <span className="truncate font-medium text-fg">
                {report.name || t('anonymous')}
              </span>
              {report.email && (
                <a
                  href={`mailto:${report.email}`}
                  className="truncate text-fg-muted hover:underline"
                >
                  {report.email}
                </a>
              )}
              <span className="ml-auto shrink-0 text-meta text-fg-meta">
                {format.relativeTime(new Date(report.created_at))}
              </span>
            </div>
            <p className="mt-1 whitespace-pre-wrap break-words text-body text-fg-secondary">
              {report.comments}
            </p>
          </li>
        ))}
      </ul>

      {hasNextPage && (
        <Button
          variant="secondary"
          size="sm"
          loading={isFetchingNextPage}
          onClick={() => fetchNextPage()}
        >
          {t('showOlder', {
            count: Math.max(last.total_count - reports.length, 0),
          })}
        </Button>
      )}
      {isFetchNextPageError && (
        <p className="text-meta text-danger-fg">{t('showOlderFailed')}</p>
      )}
    </div>
  );
}
