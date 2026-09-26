import type { Issue } from '@rustrak/client';
import { Link } from '@tanstack/react-router';
import {
  AlertCircle,
  Bookmark,
  Check,
  MoreVertical,
  Trash2,
  Volume2,
  VolumeX,
} from 'lucide-react';
import type { RefObject } from 'react';
import { useFormatter } from 'use-intl';
import type { IssueAction } from '@/features/issue/model/actions';
import {
  LevelBadge,
  PriorityIndicator,
  StatusIndicator,
} from '@/features/issue/ui/components/issue-indicators';
import { selectionColumn } from '@/shared/ui/components/data-table/columns';
import { createAppColumnHelper } from '@/shared/ui/components/data-table/use-app-table';
import { Button } from '@/shared/ui/components/shadcn/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/shared/ui/components/shadcn/dropdown-menu';
import { StopPropagation } from '@/shared/ui/components/stop-propagation';
import { TrendSparkline } from '@/shared/ui/components/trend-sparkline';

/**
 * What a cell needs from the list around it.
 *
 * Passed as a ref rather than as values so the columns can be built once per
 * project instead of once per render: the object identity never changes, and
 * the callbacks inside it are only ever read at event time, so they cannot go
 * stale the way a captured value would.
 */
export interface IssueRowHandlers {
  onAction: (action: IssueAction, issue: Issue) => void;
  onDelete: (issue: Issue) => void;
}

const helper = createAppColumnHelper<Issue>();

/**
 * The issue list's columns.
 *
 * These replace `ISSUE_COLUMNS`, a map of Tailwind widths that the header row
 * and the row component each applied by hand. Its own docblock recorded the
 * two drifting apart once already; here a header and its cells read one
 * declaration, so there is nothing left to keep in step.
 *
 * No column sorts: the table registers no sorting feature, and the order is
 * whatever the request asked the server for. The ids of `event_count` and
 * `last_seen` are nonetheless the exact strings `IssuesResource.list` accepts
 * for `sort` (the third, `digest_order`, is not a column), so if sorting is
 * added the URL, the API and the column already agree without a lookup table
 * in between.
 *
 * `t` is passed in rather than resolved with a hook: the columns are built
 * inside the list's `useMemo`, and a hook called there would break the rules
 * of hooks.
 */
/**
 * The formatter is passed in, exactly like `t`.
 *
 * `useFormatter` is a hook, and this is a builder rather than a component, so
 * it cannot reach for one itself. The component that calls it already threads
 * the translator through for the same reason.
 */
type Formatter = ReturnType<typeof useFormatter>;

export function issueColumns(
  projectId: number,
  handlers: RefObject<IssueRowHandlers>,
  t: (key: string, values?: Record<string, string | number>) => string,
  tableT: (key: string) => string,
  format: Formatter,
) {
  return helper.columns([
    selectionColumn<Issue>(tableT),

    helper.accessor('title', {
      header: t('columns.issue'),
      minSize: 280,
      meta: { grow: true },
      cell: ({ row }) => {
        const issue = row.original;
        return (
          // The title block is the link and the trailing cells are not, so a
          // reader comparing event counts can select a number without being
          // navigated away from the page they are reading.
          <Link
            to="/projects/$id/issues/$issueId"
            params={{ id: projectId, issueId: issue.id }}
            className="block py-0.5 transition-colors hover:text-primary"
          >
            <div className="mb-1 flex items-center gap-2">
              {issue.is_bookmarked && (
                <Bookmark className="size-4 shrink-0 fill-current text-primary" />
              )}
              <span className="truncate font-semibold">{issue.title}</span>
            </div>
            {issue.culprit && (
              <p className="mb-1.5 truncate font-mono text-xs text-muted-foreground/70">
                {issue.culprit}
              </p>
            )}
            <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
              <StatusIndicator issue={issue} />
              <PriorityIndicator priority={issue.priority} />
              <LevelBadge level={issue.level} />
              {issue.platform && <span>{issue.platform}</span>}
              <span className="font-mono text-muted-foreground/70">
                {issue.short_id}
              </span>
              {/* The one column a phone cannot show, folded into the title
                  block so "when did this last happen" survives the narrow
                  layout. */}
              <span className="sm:hidden">
                {format.relativeTime(new Date(issue.last_seen))}
              </span>
            </div>
          </Link>
        );
      },
    }),

    helper.accessor('trend', {
      header: t('columns.trend'),
      size: 76,
      minSize: 64,
      meta: { hideBelow: 'lg' },
      cell: ({ row }) => <TrendSparkline trend={row.original.trend ?? []} />,
    }),

    helper.accessor('first_seen', {
      id: 'age',
      header: t('columns.age'),
      size: 96,
      minSize: 80,
      meta: { align: 'end', hideBelow: 'lg' },
      cell: ({ getValue }) => (
        <span className="whitespace-nowrap text-sm text-muted-foreground">
          {format.relativeTime(new Date(getValue()))}
        </span>
      ),
    }),

    helper.accessor('event_count', {
      header: t('columns.events'),
      // Wider than the number needs, so "EVENTS" stops truncating to "EVEN…"
      // and so the header still fits once it has a sort indicator to carry.
      size: 116,
      minSize: 100,
      meta: { align: 'end', hideBelow: 'sm' },
      cell: ({ getValue }) => (
        <span className="font-mono text-sm tabular-nums">
          {format.number(getValue())}
        </span>
      ),
    }),

    helper.accessor('user_count', {
      header: t('columns.users'),
      size: 96,
      minSize: 80,
      meta: { align: 'end', hideBelow: 'lg' },
      cell: ({ getValue }) => (
        <span className="font-mono text-sm tabular-nums">
          {format.number(getValue() ?? 0)}
        </span>
      ),
    }),

    helper.accessor('last_seen', {
      header: t('columns.lastSeen'),
      size: 140,
      minSize: 110,
      meta: { align: 'end', hideBelow: 'sm' },
      cell: ({ getValue }) => (
        <span className="whitespace-nowrap text-sm text-muted-foreground">
          {format.relativeTime(new Date(getValue()))}
        </span>
      ),
    }),

    helper.display({
      id: 'actions',
      // One 32px icon button plus the cell's 8px of padding either side.
      size: 56,
      minSize: 56,
      maxSize: 56,
      header: () => <span className="sr-only">{t('actionsTitle')}</span>,
      cell: ({ row }) => {
        const issue = row.original;
        return (
          <StopPropagation>
            <DropdownMenu>
              <DropdownMenuTrigger
                render={
                  <Button
                    variant="ghost"
                    size="icon"
                    className="size-8"
                    aria-label={t('actionsFor', { title: issue.title })}
                  />
                }
              >
                <MoreVertical className="size-4" />
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                {issue.status !== 'resolved' && (
                  <DropdownMenuItem
                    onClick={() => handlers.current.onAction('resolve', issue)}
                  >
                    <Check className="mr-2 size-4" />
                    {t('actions.resolve')}
                  </DropdownMenuItem>
                )}
                {issue.status === 'resolved' && (
                  <DropdownMenuItem
                    onClick={() =>
                      handlers.current.onAction('unresolve', issue)
                    }
                  >
                    <AlertCircle className="mr-2 size-4" />
                    {t('actions.unresolve')}
                  </DropdownMenuItem>
                )}
                {issue.status !== 'ignored' && (
                  <DropdownMenuItem
                    onClick={() => handlers.current.onAction('mute', issue)}
                  >
                    <VolumeX className="mr-2 size-4" />
                    {t('actions.mute')}
                  </DropdownMenuItem>
                )}
                {issue.status === 'ignored' && (
                  <DropdownMenuItem
                    onClick={() => handlers.current.onAction('unmute', issue)}
                  >
                    <Volume2 className="mr-2 size-4" />
                    {t('actions.unmute')}
                  </DropdownMenuItem>
                )}
                <DropdownMenuItem
                  variant="destructive"
                  onClick={() => handlers.current.onDelete(issue)}
                >
                  <Trash2 className="mr-2 size-4" />
                  {t('actions.delete')}
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </StopPropagation>
        );
      },
    }),
  ]);
}
