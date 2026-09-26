import type { Issue } from '@rustrak/client';
import { CircleAlert } from 'lucide-react';
import { useFormatter, useTranslations } from 'use-intl';
import { IssueActions } from '@/features/issue/ui/components/issue-actions';
import { StatusIndicator } from '@/features/issue/ui/components/issue-indicators';
import { cn } from '@/shared/lib/utils';
import { Link } from '@/shared/ui/components/link';

/**
 * The band an event opens with: what broke, where, and what can be done
 * about it.
 *
 * The workflow row shares the header's elevated surface rather than sitting
 * in the body, because resolving or muting acts on the issue named directly
 * above it and not on the event being read below.
 */
export function EventHeader({
  issue,
  projectId,
  titleType,
  message,
  levelText,
  userCount,
}: {
  issue: Issue;
  projectId: number;
  titleType: string;
  message: string;
  levelText: string;
  userCount: number;
}) {
  const t = useTranslations('projectPages');
  const format = useFormatter();

  return (
    <header className="shrink-0 bg-card border-b">
      <div className="w-full px-4 md:px-8 py-3 space-y-1.5">
        <nav className="flex items-center gap-1.5 text-xs text-muted-foreground min-w-0">
          <Link
            href={`/projects/${projectId}/issues`}
            className="hover:text-foreground transition-colors"
          >
            {t('issues.title')}
          </Link>
          <span className="text-muted-foreground/40">/</span>
          <span className="font-mono text-foreground truncate">
            {issue.short_id}
          </span>
        </nav>

        <div className="flex items-start justify-between gap-6">
          <h1 className="text-lg sm:text-xl font-semibold tracking-tight truncate min-w-0">
            {titleType}
          </h1>
          <div className="flex items-start gap-4 sm:gap-8 shrink-0">
            <div className="text-right">
              <p className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                {t('event.eventsTotal')}
              </p>
              <p className="text-xl font-semibold tabular-nums leading-tight">
                {format.number(issue.event_count, 'compact')}
              </p>
            </div>
            <div className="text-right">
              <p className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                {t('event.users')}
              </p>
              <p className="text-xl font-semibold tabular-nums leading-tight">
                {format.number(userCount, 'compact')}
              </p>
            </div>
          </div>
        </div>

        <div className="flex items-start gap-2 text-sm text-muted-foreground min-w-0">
          <CircleAlert className={cn('size-4 shrink-0 mt-0.5', levelText)} />
          <p className="truncate font-mono text-foreground/90">{message}</p>
        </div>

        <div className="flex items-center gap-2 text-sm text-muted-foreground min-w-0">
          <StatusIndicator issue={issue} />
          {issue.culprit && (
            <span className="font-mono truncate">{issue.culprit}</span>
          )}
        </div>
      </div>

      {/* Workflow toolbar — same elevated band as the header */}
      <div className="border-t">
        <div className="w-full px-4 md:px-8 py-2">
          <IssueActions issue={issue} projectId={projectId} />
        </div>
      </div>
    </header>
  );
}
