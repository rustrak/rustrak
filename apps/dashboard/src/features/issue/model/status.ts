import type {
  IssuePriority,
  IssueResponse,
  IssueStatus,
} from '@rustrak/client';

/**
 * Presentation metadata for an issue indicator (#165).
 *
 * The UI favors restraint over loud color: status and priority are shown as a
 * small leading dot plus a muted text label, the way Sentry surfaces them —
 * color is a quiet signal, not the dominant visual element.
 *
 * `labelKey` is a message key resolved by the rendering component, so this
 * portable module never holds user-facing sentences.
 */
export interface IssueIndicator {
  labelKey: string;
  /** Tailwind `bg-*` class for the small leading dot. */
  dot: string;
}

const SUBSTATUS_KEYS: Record<string, string> = {
  new: 'issueStatus.new',
  ongoing: 'issueStatus.ongoing',
  escalating: 'issueStatus.escalating',
  regressed: 'issueStatus.regressed',
  archived_until_escalating: 'issueStatus.archived',
  archived_until_condition_met: 'issueStatus.archived',
  archived_forever: 'issueStatus.archived',
};

/**
 * Resolve the dot color + label key for an issue's status.
 *
 * Prefers a recognized substatus label, falling back to the coarse status.
 * Unresolved (the common case) stays neutral; only resolved earns an accent.
 */
export function statusDisplay(
  issue: Pick<IssueResponse, 'status' | 'substatus'>,
): IssueIndicator {
  const substatusKey = issue.substatus
    ? SUBSTATUS_KEYS[issue.substatus]
    : undefined;

  switch (issue.status) {
    case 'resolved':
      return { labelKey: 'issueStatus.resolved', dot: 'bg-success' };
    case 'ignored':
      return {
        labelKey: substatusKey ?? 'issueStatus.ignored',
        dot: 'bg-fg-disabled',
      };
    default:
      return {
        labelKey: substatusKey ?? 'issueStatus.unresolved',
        dot: 'bg-fg-subtle',
      };
  }
}

/**
 * The tag tone for an issue's status, the same on every screen: open is the
 * one that asks for attention; resolved and archived are both done with.
 */
export const STATUS_TONE = {
  unresolved: 'warning',
  resolved: 'neutral',
  ignored: 'neutral',
} as const satisfies Record<IssueStatus, string>;

const PRIORITY_META: Record<IssuePriority, IssueIndicator> = {
  high: { labelKey: 'issueStatus.high', dot: 'bg-sev-error' },
  medium: { labelKey: 'issueStatus.medium', dot: 'bg-sev-warning' },
  low: { labelKey: 'issueStatus.low', dot: 'bg-chart-1' },
};

/**
 * Dot color + label key for a priority tier, or `null` when unset.
 */
export function priorityDisplay(
  priority: IssuePriority | null | undefined,
): IssueIndicator | null {
  if (!priority) {
    return null;
  }
  return PRIORITY_META[priority] ?? null;
}
