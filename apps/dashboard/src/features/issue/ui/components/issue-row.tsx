import type { IssueResponse } from '@rustrak/client';
import { cn, Sparkline } from '@rustrak/ui';
import { useFormatter, useTranslations } from 'use-intl';
import { levelMark } from '@/shared/config/levels';
import { Link } from '@/shared/ui/components/link';

/** One issue in a panel's short list, linked to its page. */
export function IssueRow({
  projectId,
  issue,
}: {
  projectId: number;
  issue: IssueResponse;
}) {
  const t = useTranslations('projectPages.overview');
  const format = useFormatter();
  return (
    <li>
      <Link
        href={`/projects/${projectId}/issues/${issue.id}`}
        className="flex items-center gap-3 px-5 py-2.5 hover:bg-surface-hover"
      >
        <span
          aria-hidden="true"
          className={cn(
            'h-8 w-0.75 shrink-0 rounded-pill',
            levelMark(issue.level),
          )}
        />
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="truncate text-value font-medium text-fg">
            {issue.title}
          </span>
          <span className="truncate text-hint text-fg-subtle">
            <span className="font-mono">{issue.short_id}</span>
            {issue.culprit ? ` · ${issue.culprit}` : null}
          </span>
        </span>
        {issue.trend ? (
          <Sparkline
            values={issue.trend}
            label={t('issueTrend')}
            className="hidden h-6 w-20 shrink-0 sm:block"
          />
        ) : null}
        <span className="flex w-24 shrink-0 flex-col items-end">
          <span className="text-value font-medium text-fg tabular-nums">
            {format.number(issue.event_count, 'compact')}
          </span>
          <span className="truncate text-hint text-fg-meta">
            {format.relativeTime(new Date(issue.last_seen))}
          </span>
        </span>
      </Link>
    </li>
  );
}
