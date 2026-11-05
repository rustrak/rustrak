import type { ActivityEntry } from '@rustrak/client';
import { Button, CommentIcon, cn, Textarea } from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { toast } from 'sonner';
import { useFormatter, useTranslations } from 'use-intl';
import { issueMutations } from '@/features/issue/api/mutations';
import { GitHubMark } from '@/shared/ui/components/github-mark';
import { usePermissionsLater } from '@/shared/ui/hooks/use-permissions';

interface IssueActivityProps {
  projectId: number;
  issueId: string;
  activity: ActivityEntry[];
}

function parseData(data: string): Record<string, unknown> {
  try {
    const parsed = JSON.parse(data);
    return parsed && typeof parsed === 'object' ? parsed : {};
  } catch {
    return {};
  }
}

/** The text body for a comment/note entry. */
function noteText(entry: ActivityEntry): string {
  const text = parseData(entry.data).text;
  return typeof text === 'string' ? text : entry.data;
}

/** What GitHub has to do with an entry: who did it there, and where. */
function githubSource(
  entry: ActivityEntry,
): { author: string; url: string } | null {
  const d = parseData(entry.data);
  const commit = d.commit as Record<string, unknown> | undefined;
  if (d.source !== 'github') return null;
  return {
    author: String(commit?.author ?? d.author ?? ''),
    url: String(commit?.url ?? d.url ?? ''),
  };
}

type Translate = (
  key: string,
  values?: Record<string, string | number>,
) => string;

/** A status change, made in Rustrak, on GitHub, or by a commit. */
function describeStatus(d: Record<string, unknown>, t: Translate): string {
  const status = typeof d.status === 'string' ? d.status : '';
  const commit = d.commit as Record<string, unknown> | undefined;
  if (commit) {
    return t('activity.resolvedByCommit', {
      sha: String(commit.sha ?? '').slice(0, 7),
    });
  }
  if (d.source === 'github') {
    return status === 'resolved'
      ? t('activity.closedOnGitHub')
      : t('activity.reopenedOnGitHub');
  }
  if (status === 'resolved') return t('activity.statusResolved');
  if (status === 'unresolved') return t('activity.statusReopened');
  if (status === 'ignored') return t('activity.statusMuted');
  return t('activity.statusChanged', { status });
}

function describeGitHubLink(d: Record<string, unknown>, t: Translate): string {
  const ref = `${String(d.repo ?? '')}#${String(d.number ?? '')}`;
  if (d.action === 'created') return t('activity.githubCreated', { ref });
  if (d.action === 'unlinked') return t('activity.githubUnlinked', { ref });
  return t('activity.githubLinked', { ref });
}

/** A human-readable description of a non-note activity entry. */
function describe(entry: ActivityEntry, t: Translate): string {
  const d = parseData(entry.data);
  switch (entry.type) {
    case 'set_status':
      return describeStatus(d, t);
    case 'github_link':
      return describeGitHubLink(d, t);
    case 'set_priority':
      return t('activity.priorityChanged', {
        priority: String(d.priority ?? '—'),
      });
    case 'set_regression':
      return t('activity.regression');
    case 'first_seen':
      return t('activity.firstSeen');
    default:
      return entry.type.replace(/_/g, ' ');
  }
}

export function IssueActivity({
  projectId,
  issueId,
  activity,
}: IssueActivityProps) {
  const format = useFormatter();
  const t = useTranslations('issues');
  // Commenting is `issues:update` on the server.
  const canComment = usePermissionsLater().can('issues', 'update', projectId);
  const [text, setText] = useState('');
  const { mutate, isPending } = useMutation({
    ...issueMutations.comment(projectId, issueId),
    onSuccess: () => setText(''),
    // Keep the text in the box: clearing it on a failure destroys what the
    // user wrote and leaves no trace that anything went wrong.
    onError: (error) =>
      toast.error(t('activity.commentFailed'), {
        description: error.message,
      }),
  });

  const entries = [...activity].sort(
    (a, b) =>
      new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
  );

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const body = text.trim();
    if (!body) {
      return;
    }
    mutate(body);
  };

  return (
    <div className="flex flex-col gap-4">
      {canComment ? (
        <form onSubmit={handleSubmit} className="flex flex-col gap-2">
          <Textarea
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={t('activity.placeholder')}
            rows={3}
            disabled={isPending}
            aria-label={t('activity.placeholder')}
          />
          <div className="flex justify-end">
            <Button
              type="submit"
              size="sm"
              variant="secondary"
              icon={CommentIcon}
              loading={isPending}
              disabled={!text.trim()}
            >
              {t('activity.comment')}
            </Button>
          </div>
        </form>
      ) : null}

      {entries.length === 0 ? (
        <p className="py-4 text-center text-meta text-fg-subtle">
          {t('activity.empty')}
        </p>
      ) : (
        // A timeline: one rule down the left, a mark per entry on it. A
        // comment is set apart in its own box -- it is what people said,
        // the rest is what happened.
        <ol className="relative flex flex-col gap-4 before:absolute before:inset-y-1 before:left-[3px] before:w-px before:bg-border-divider before:content-['']">
          {entries.map((entry) => {
            const isNote = entry.type === 'note' || entry.type === 'comment';
            const github = githubSource(entry);
            return (
              <li key={entry.id} className="relative flex gap-3 ps-0">
                <span
                  aria-hidden="true"
                  className={cn(
                    'relative mt-1.5 size-2 shrink-0 rounded-pill ring-2 ring-panel',
                    isNote ? 'bg-fg-secondary' : 'bg-fg-ghost',
                  )}
                />
                <div className="flex min-w-0 flex-1 flex-col gap-1">
                  {github ? (
                    <a
                      href={github.url || undefined}
                      target="_blank"
                      rel="noreferrer"
                      className="inline-flex items-center gap-1 text-hint text-fg-subtle hover:text-fg"
                    >
                      <GitHubMark className="size-3" />
                      {github.author || 'GitHub'}
                    </a>
                  ) : null}
                  {isNote ? (
                    <p className="whitespace-pre-wrap break-words rounded-md border border-border-subtle bg-surface px-3 py-2 text-value text-fg">
                      {noteText(entry)}
                    </p>
                  ) : (
                    <p className="text-value text-fg-secondary">
                      {describe(entry, t)}
                    </p>
                  )}
                  <time
                    dateTime={entry.created_at}
                    title={format.dateTime(
                      new Date(entry.created_at),
                      'dateTime',
                    )}
                    className="text-hint text-fg-meta"
                  >
                    {format.relativeTime(new Date(entry.created_at))}
                  </time>
                </div>
              </li>
            );
          })}
        </ol>
      )}
    </div>
  );
}
