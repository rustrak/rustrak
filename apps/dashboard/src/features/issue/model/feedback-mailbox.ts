import type { IssueStatus, UpdateIssueState } from '@rustrak/client';

/**
 * Sentry's three feedback mailboxes, each an issue status: Spam is a muted
 * (`ignored`) feedback, as in Sentry's own UI.
 */
export const MAILBOXES = ['inbox', 'resolved', 'spam'] as const;
export type Mailbox = (typeof MAILBOXES)[number];

/** The issue list filter a mailbox reads. */
export const MAILBOX_FILTER = {
  inbox: 'open',
  resolved: 'resolved',
  spam: 'muted',
} as const satisfies Record<Mailbox, string>;

/**
 * Mark as spam, or move spam back to the inbox. Sentry's feedback actions send
 * a bare `ignored` with no substatus, and so does the issue page's archive.
 */
export function toggleSpam(status: IssueStatus): UpdateIssueState {
  return status === 'ignored'
    ? { status: 'unresolved' }
    : { status: 'ignored' };
}

/** Who sent it, by the most personal thing they gave; empty if nothing. */
export function feedbackSender(feedback: {
  name: string;
  contact_email: string;
}): string {
  return feedback.name || feedback.contact_email;
}

/** A page address without its scheme, which every row would repeat. */
export function shortUrl(url: string): string {
  return url.replace(/^[a-z]+:\/\//i, '');
}

/**
 * Which row the inbox opens after `id` leaves it -- resolved or sent to
 * spam -- so triage carries on down the list: the next one, or the one
 * before when it was the last.
 */
export function nextAfter(ids: string[], id: string): string | undefined {
  const index = ids.indexOf(id);
  if (index === -1) return undefined;
  return ids[index + 1] ?? ids[index - 1];
}

/**
 * Where the inbox lands after `id` leaves it: the neighbour on the same page,
 * or the page before when that was its last row -- the emptied page would
 * show nothing, not even the controls to leave it.
 */
export function landingAfter(
  ids: string[],
  id: string,
  page: number,
): { page: number; selected: string | undefined } {
  const selected = nextAfter(ids, id);
  return { page: selected || page === 1 ? page : page - 1, selected };
}

/**
 * Whether a page address the SDK sent may be a link. It comes from whoever
 * holds the DSN, so any other scheme -- `mailto:`, an app's own -- stays text.
 */
export function isWebUrl(url: string): boolean {
  return /^https?:\/\//i.test(url);
}
