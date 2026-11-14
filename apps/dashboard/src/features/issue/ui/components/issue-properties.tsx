import type { IssueResponse } from '@rustrak/client';
import { Avatar, Button, cn, Menu, type MenuAction, Text } from '@rustrak/ui';
import type { ReactNode } from 'react';
import { useFormatter, useTranslations } from 'use-intl';
import { PRIORITIES } from '@/features/issue/model/actions';
import type { Assignable } from '@/features/issue/model/assignable';
import { priorityDisplay, statusDisplay } from '@/features/issue/model/status';
import { useRelativeTime } from '@/shared/i18n/use-now';
import { useIssueChange } from './issue-actions';

export type { Assignable } from '@/features/issue/model/assignable';

/** Dot colours by meaning, from the design system's severity scale. */
const PRIORITY_DOT: Record<string, string> = {
  high: 'bg-sev-error',
  medium: 'bg-sev-warning',
  low: 'bg-sev-info',
};

/**
 * The issue's properties, as a list of label and value -- Linear's sidebar.
 *
 * The values that can change are their own pickers: click the priority to
 * set it, the assignee to hand it on. This list is where the page grows: a
 * new property is a new row here, never a new button in the header.
 */
export function IssueProperties({
  issue,
  projectId,
  members,
  me,
  userCount,
}: {
  issue: IssueResponse;
  projectId: number;
  members: Assignable[];
  me: number;
  userCount: number;
}) {
  const t = useTranslations('issues');
  const format = useFormatter();
  const relativeTime = useRelativeTime();
  const { isPending, update, canUpdate } = useIssueChange(projectId, issue);
  const status = statusDisplay(issue);
  const priority = priorityDisplay(issue.priority);
  const assignee = members.find((member) => member.id === issue.assigned_to);

  const priorityActions: MenuAction[] = PRIORITIES.map((value) => ({
    id: value,
    label: t(`issueStatus.${value}`),
    leading: (
      <span
        aria-hidden="true"
        className={cn('size-2 shrink-0 rounded-pill', PRIORITY_DOT[value])}
      />
    ),
    selected: issue.priority === value,
    onSelect: () => update({ priority: value }, t('toasts.priorityFailed')),
  }));

  const assigneeActions: MenuAction[] = [
    {
      id: 'me',
      label: t('table.assignToMe'),
      selected: issue.assigned_to === me,
      onSelect: () => update({ assigned_to: me }),
    },
    {
      id: 'none',
      label: t('table.unassign'),
      disabled: issue.assigned_to == null,
      onSelect: () => update({ assigned_to: null }),
    },
    ...members
      .filter((member) => member.id !== me)
      .map((member, index) => ({
        id: String(member.id),
        label: member.email,
        leading: <Avatar name={member.email} size="xs" />,
        selected: issue.assigned_to === member.id,
        separated: index === 0,
        onSelect: () => update({ assigned_to: member.id }),
      })),
  ];

  const seen = (iso: string) => (
    <span title={format.dateTime(new Date(iso), 'dateTime')}>
      {relativeTime(new Date(iso))}
    </span>
  );

  return (
    <dl className="flex flex-col">
      <Property label={t('detail.status')}>
        <span className="inline-flex items-center gap-2 text-value text-fg">
          <span
            aria-hidden="true"
            className={cn(
              'size-2 shrink-0 rounded-pill',
              issue.status === 'resolved'
                ? 'bg-success'
                : issue.status === 'ignored'
                  ? 'bg-fg-ghost'
                  : 'bg-sev-warning',
            )}
          />
          {t(status.labelKey)}
        </span>
      </Property>

      <Property label={t('priority')}>
        <Menu
          align="end"
          actions={priorityActions}
          trigger={
            <PropertyButton disabled={isPending || !canUpdate}>
              {priority && issue.priority ? (
                <>
                  <span
                    aria-hidden="true"
                    className={cn(
                      'size-2 shrink-0 rounded-pill',
                      PRIORITY_DOT[issue.priority],
                    )}
                  />
                  {t(priority.labelKey)}
                </>
              ) : (
                <Text variant="value" tone="ghost">
                  {t('set')}
                </Text>
              )}
            </PropertyButton>
          }
        />
      </Property>

      <Property label={t('columns.assignee')}>
        <Menu
          align="end"
          actions={assigneeActions}
          popupClassName="min-w-56"
          trigger={
            <PropertyButton disabled={isPending || !canUpdate}>
              {assignee ? (
                <>
                  <Avatar name={assignee.email} size="xs" />
                  {/* A fixed cap rather than shrink-to-fit: inside the button a
                      shrinkable label collapses to nothing. */}
                  <span className="max-w-40 truncate">
                    {assignee.id === me ? t('table.me') : assignee.email}
                  </span>
                </>
              ) : (
                <Text variant="value" tone="ghost">
                  {t('table.unassigned')}
                </Text>
              )}
            </PropertyButton>
          }
        />
      </Property>

      <div className="my-2 h-px bg-border-divider" />

      <Property label={t('columns.events')}>
        <span className="text-value text-fg tabular-nums">
          {format.number(issue.event_count)}
        </span>
      </Property>
      <Property label={t('columns.users')}>
        <span className="text-value text-fg tabular-nums">
          {format.number(userCount)}
        </span>
      </Property>

      <div className="my-2 h-px bg-border-divider" />

      <Property label={t('columns.firstSeen')}>
        <SeenValue
          when={seen(issue.first_seen)}
          release={issue.first_release}
        />
      </Property>
      <Property label={t('columns.lastSeen')}>
        <SeenValue when={seen(issue.last_seen)} release={issue.last_release} />
      </Property>
    </dl>
  );
}

function Property({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex min-h-9 items-center justify-between gap-4">
      <dt className="shrink-0 text-meta text-fg-meta">{label}</dt>
      <dd className="flex min-w-0 justify-end text-value text-fg">
        {children}
      </dd>
    </div>
  );
}

/** A value that is also its own picker: flat until hovered. */
function PropertyButton({
  children,
  disabled,
  ...props
}: {
  children: ReactNode;
  disabled?: boolean;
}) {
  return (
    <Button
      variant="ghost"
      size="sm"
      menu
      disabled={disabled}
      className="-me-2 max-w-full text-fg"
      {...props}
    >
      <span className="flex items-center gap-2">{children}</span>
    </Button>
  );
}

function SeenValue({ when, release }: { when: ReactNode; release: string }) {
  return (
    <span className="flex min-w-0 flex-col items-end">
      <span className="text-value text-fg">{when}</span>
      {release ? (
        <Text variant="mono-sm" tone="meta" truncate className="max-w-44">
          {release}
        </Text>
      ) : null}
    </span>
  );
}
