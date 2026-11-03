import type { IssueGitHub } from '@rustrak/client';
import { CommitIcon } from '@rustrak/ui';
import { useFormatter, useTranslations } from 'use-intl';
import { GitHubMark } from '@/shared/ui/components/github-mark';
import { Link } from '@/shared/ui/components/link';

/**
 * The rail's "Code" block: the commits that may have caused the issue and who
 * owns the file it broke in.
 *
 * Without a repository it is one line pointing at the setting that turns it
 * on; with one and nothing to say, it is absent.
 */
export function IssueCodeBlock({
  projectId,
  code,
  connected,
}: {
  projectId: number;
  /** `null` when it could not be loaded, which is not "no repository". */
  code: IssueGitHub | null;
  /** `null` when it is not known: a failed request is not "disconnected". */
  connected: boolean | null;
}) {
  const t = useTranslations('github');
  const format = useFormatter();

  if (connected === false) {
    return (
      <div>
        <Link
          href={`/projects/${projectId}/settings/github`}
          className="flex items-center gap-2 text-meta text-fg-subtle hover:text-fg"
        >
          <GitHubMark className="size-3.5" />
          {t('code.connect')}
        </Link>
      </div>
    );
  }

  if (
    !code ||
    (code.suspect_commits.length === 0 && code.owners.length === 0)
  ) {
    return null;
  }

  return (
    <div className="space-y-3">
      {code.suspect_commits.length > 0 && (
        <div className="space-y-2">
          <p className="text-xs text-fg-subtle">{t('code.suspect')}</p>
          {code.suspect_commits.map((commit) => (
            <a
              key={commit.sha}
              href={commit.url}
              target="_blank"
              rel="noreferrer"
              className="flex gap-2 rounded-md p-1.5 -mx-1.5 hover:bg-surface-hover"
            >
              <CommitIcon className="size-4 mt-0.5 shrink-0 text-fg-subtle" />
              <div className="min-w-0 text-sm">
                <p className="truncate">{commit.message.split('\n')[0]}</p>
                <p className="text-xs text-fg-subtle truncate">
                  <span className="font-mono">{commit.sha.slice(0, 7)}</span>
                  {' · '}
                  {commit.author_login ?? commit.author_name}
                  {' · '}
                  {format.relativeTime(new Date(commit.committed_at))}
                </p>
              </div>
            </a>
          ))}
        </div>
      )}

      {code.owners.length > 0 && (
        <div className="flex items-baseline justify-between gap-2 text-sm">
          <span className="text-fg-subtle">{t('code.owners')}</span>
          <span className="font-mono text-xs text-right">
            {code.owners.join(', ')}
          </span>
        </div>
      )}
    </div>
  );
}
