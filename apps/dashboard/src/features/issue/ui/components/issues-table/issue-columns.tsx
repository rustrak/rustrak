import type { IssueResponse } from '@rustrak/client';
import {
  AssignIcon,
  Avatar,
  cn,
  createDataTableColumnHelper,
  type DataTableColumnDef,
  type FilterOption,
  Sparkline,
  Tag,
  type TagTone,
  Text,
  TimeIcon,
} from '@rustrak/ui';
import { useNow, type useTranslations } from 'use-intl';
import type { Assignable } from '@/features/issue/model/assignable';
import { SEEN_WINDOWS } from '@/features/issue/model/issue-table';
import { Link } from '@/shared/ui/components/link';
import { RelativeTime } from '@/shared/ui/components/relative-time';
import type { ColumnFormatter } from '@/shared/ui/hooks/use-column-formatter';

export type { Assignable } from '@/features/issue/model/assignable';

const LEVEL_MARK: Record<string, string> = {
  fatal: 'bg-sev-error',
  error: 'bg-sev-error',
  warning: 'bg-sev-warning',
};

const LEVEL_TONE: Record<string, TagTone> = {
  fatal: 'error',
  error: 'error',
  warning: 'warning',
  info: 'info',
};

const PRIORITY_TONE: Record<string, TagTone> = {
  high: 'error',
  medium: 'warning',
  low: 'neutral',
};

/** The badges the second line carries, worth reading before the counts. */
const SUBSTATUS_TONE: Record<string, TagTone> = {
  new: 'brand',
  regressed: 'warning',
  escalating: 'error',
};

/** An issue first seen within this long is new to the reader. */
const NEW_FOR_MS = 7 * 24 * 60 * 60 * 1000;

interface ColumnContext {
  projectId: number;
  t: ReturnType<typeof useTranslations<'issues'>>;
  format: ColumnFormatter;
  members: Assignable[];
  me: number;
}

const column = createDataTableColumnHelper<IssueResponse>();

/**
 * The issue table's columns.
 *
 * One identity column carries what an issue *is* -- its level as a stripe,
 * its title, and under it the short id, where it fails and what is unusual
 * about it now (new, regressed, escalating). Every other column is a figure
 * you sort or filter by, at a fixed width the reader can drag.
 */
export function issueColumns({
  projectId,
  t,
  format,
  members,
  me,
}: ColumnContext): DataTableColumnDef<IssueResponse>[] {
  const seenOptions: FilterOption[] = SEEN_WINDOWS.map((value) => ({
    value,
    label: t(`table.windows.${value}`),
  }));

  return column.columns([
    column.accessor('title', {
      id: 'issue',
      header: t('columns.issue'),
      enableHiding: false,
      enableSorting: false,
      cell: ({ row }) => (
        <IssueCell
          issue={row.original}
          href={`/projects/${projectId}/issues/${row.original.id}`}
          t={t}
        />
      ),
      meta: { label: t('columns.issue') },
    }),
    column.accessor('level', {
      id: 'level',
      header: t('columns.level'),
      enableSorting: false,
      cell: ({ getValue }) => {
        const level = getValue() ?? 'error';
        return (
          <Tag variant="soft" tone={LEVEL_TONE[level] ?? 'neutral'}>
            {level}
          </Tag>
        );
      },
      meta: {
        label: t('columns.level'),
        width: 96,
        filter: {
          variant: 'options',
          options: (['fatal', 'error', 'warning', 'info'] as const).map(
            (value) => ({
              value,
              label: t(`table.levels.${value}`),
              tone: LEVEL_TONE[value],
            }),
          ),
        },
      },
    }),
    column.accessor('trend', {
      id: 'trend',
      header: t('columns.trend'),
      enableSorting: false,
      cell: ({ getValue }) => {
        const trend = getValue();
        return trend?.length ? (
          <Sparkline
            values={trend}
            label={t('columns.trend')}
            className="h-6 w-full"
          />
        ) : null;
      },
      meta: { label: t('columns.trend'), width: 120 },
    }),
    column.accessor('event_count', {
      id: 'events',
      header: t('columns.events'),
      cell: ({ getValue }) => format.number(getValue(), 'compact'),
      meta: {
        label: t('columns.events'),
        width: 92,
        align: 'end',
        numeric: true,
        sortLabels: { asc: t('table.fewest'), desc: t('table.most') },
        filter: { variant: 'range', min: 0 },
      },
    }),
    column.accessor('user_count', {
      id: 'users',
      header: t('columns.users'),
      enableSorting: false,
      cell: ({ getValue }) => format.number(getValue() ?? 0, 'compact'),
      meta: {
        label: t('columns.users'),
        width: 84,
        align: 'end',
        numeric: true,
      },
    }),
    column.accessor('priority', {
      id: 'priority',
      header: t('priority'),
      cell: ({ getValue }) => {
        const priority = getValue();
        return priority ? (
          <Tag variant="soft" tone={PRIORITY_TONE[priority] ?? 'neutral'}>
            {t(`issueStatus.${priority}`)}
          </Tag>
        ) : null;
      },
      meta: {
        label: t('priority'),
        width: 104,
        sortLabels: {
          asc: t('table.lowestPriority'),
          desc: t('table.highestPriority'),
        },
        filter: {
          variant: 'options',
          options: (['high', 'medium', 'low'] as const).map((value) => ({
            value,
            label: t(`issueStatus.${value}`),
            tone: PRIORITY_TONE[value],
          })),
        },
      },
    }),
    column.accessor('assigned_to', {
      id: 'assignee',
      header: t('columns.assignee'),
      enableSorting: false,
      cell: ({ getValue }) => (
        <AssigneeCell
          assignedTo={getValue()}
          member={members.find((member) => member.id === getValue())}
          me={me}
          t={t}
        />
      ),
      meta: {
        label: t('columns.assignee'),
        icon: AssignIcon,
        width: 148,
        filter: {
          variant: 'options',
          multiple: false,
          options: [
            { value: String(me), label: t('table.me') },
            { value: 'none', label: t('table.unassigned') },
            ...members
              .filter((member) => member.id !== me)
              .map((member) => ({
                value: String(member.id),
                label: member.email,
              })),
          ],
        },
      },
    }),
    column.accessor('last_seen', {
      id: 'lastSeen',
      header: t('columns.lastSeen'),
      cell: ({ getValue }) => (
        <span title={format.dateTime(new Date(getValue()), 'precise')}>
          <RelativeTime at={getValue()} style="narrow" />
        </span>
      ),
      meta: {
        label: t('columns.lastSeen'),
        icon: TimeIcon,
        width: 120,
        align: 'end',
        sortLabels: { asc: t('table.oldest'), desc: t('table.newest') },
        filter: { variant: 'options', multiple: false, options: seenOptions },
      },
    }),
    column.accessor('first_seen', {
      id: 'firstSeen',
      header: t('columns.age'),
      cell: ({ getValue }) => (
        <span title={format.dateTime(new Date(getValue()), 'precise')}>
          <RelativeTime at={getValue()} style="narrow" />
        </span>
      ),
      meta: {
        label: t('columns.age'),
        icon: TimeIcon,
        width: 112,
        align: 'end',
        sortLabels: { asc: t('table.oldest'), desc: t('table.newest') },
        filter: { variant: 'options', multiple: false, options: seenOptions },
      },
    }),
  ]);
}

/**
 * The one thing unusual about an issue right now, if any: closed (outside
 * the open view), escalating, regressed, or new to the reader.
 */
function issueBadge(
  issue: IssueResponse,
  now: number,
): { key: string; tone: TagTone } | null {
  if (issue.status === 'resolved' || issue.status === 'ignored') {
    return { key: issue.status, tone: 'neutral' };
  }
  const tone = issue.substatus ? SUBSTATUS_TONE[issue.substatus] : undefined;
  if (issue.substatus && tone) return { key: issue.substatus, tone };
  if (now - new Date(issue.first_seen).getTime() < NEW_FOR_MS) {
    return { key: 'new', tone: 'brand' };
  }
  return null;
}

/** Title, then the short id, where it fails and what is unusual about it. */
function IssueCell({
  issue,
  href,
  t,
}: {
  issue: IssueResponse;
  /** The title is a real link, so it opens in a new tab like any other. */
  href: string;
  t: ColumnContext['t'];
}) {
  const now = useNow().getTime();
  const badge = issueBadge(issue, now);

  return (
    <span className="flex min-w-0 items-center gap-3 py-2">
      <span
        aria-hidden="true"
        className={cn(
          'h-8 w-0.75 shrink-0 rounded-pill',
          LEVEL_MARK[issue.level ?? ''] ?? 'bg-sev-info',
        )}
      />
      <span className="flex min-w-0 flex-col gap-0.5">
        <span className="flex min-w-0 items-center gap-2">
          {/* An issue the reader has not opened yet reads brighter. */}
          {/* The row opens the issue on a click anywhere; the title is also
              a link, so a middle-click or "open in new tab" works on it. The
              row ignores clicks on links, so the two never both navigate. */}
          <Link
            href={href}
            className={cn(
              'min-w-0 truncate text-value font-medium hover:underline',
              issue.has_seen === false ? 'text-fg' : 'text-fg-secondary',
            )}
          >
            {issue.title}
          </Link>
          {badge ? (
            <Tag variant="soft" tone={badge.tone} className="shrink-0">
              {t(`issueStatus.${badge.key}`)}
            </Tag>
          ) : null}
        </span>
        <Text variant="mono-sm" tone="ghost" truncate>
          {issue.short_id}
          {issue.culprit ? ` · ${issue.culprit}` : ''}
        </Text>
      </span>
    </span>
  );
}

function AssigneeCell({
  assignedTo,
  member,
  me,
  t,
}: {
  assignedTo: number | null | undefined;
  member: Assignable | undefined;
  me: number;
  t: ColumnContext['t'];
}) {
  // Nobody is the common case; a word in every row would be the loudest
  // thing in the column.
  if (assignedTo == null) {
    return (
      <span className="text-fg-ghost" title={t('table.unassigned')}>
        –
      </span>
    );
  }
  // Assigned to someone the member list cannot name: it has not loaded, did
  // not load, or they have left the project. Still assigned, never "–".
  if (!member) {
    return (
      <Text variant="meta" tone="subtle" truncate>
        {assignedTo === me ? t('table.me') : t('table.assigned')}
      </Text>
    );
  }
  return (
    <span className="flex min-w-0 items-center gap-2">
      <Avatar name={member.email} size="xs" />
      <Text variant="meta" truncate>
        {member.id === me ? t('table.me') : member.email.split('@')[0]}
      </Text>
    </span>
  );
}
