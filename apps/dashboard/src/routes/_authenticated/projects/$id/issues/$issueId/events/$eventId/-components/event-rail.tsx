import type {
  ActivityEntry,
  IssueAggregates,
  IssueGitHub,
  IssueResponse,
} from '@rustrak/client';
import { useTranslations } from 'use-intl';
import { IssueCodeBlock } from '@/features/github/ui/components/issue-code-block';
import { IssueActivity } from '@/features/issue/ui/components/issue-activity';
import {
  type Assignable,
  IssueProperties,
} from '@/features/issue/ui/components/issue-properties';
import { TagDistribution } from '@/features/issue/ui/components/tag-distribution';
import { RailBlock } from '@/shared/ui/components/panel';

/**
 * The rail: everything about the **issue**, beside the event being read.
 *
 * Ordered by how often it changes the reader's next move: who owns it and
 * how urgent it is, how widely it reaches, where its code lives, and what
 * has been said about it. A new kind of fact about an issue is a new block
 * here; the reading column stays the event's.
 */
export function EventRail({
  projectId,
  issueId,
  issue,
  aggregates,
  activity,
  code,
  connected,
  members,
  me,
}: {
  projectId: number;
  issueId: string;
  issue: IssueResponse;
  aggregates: IssueAggregates | null;
  activity: ActivityEntry[];
  /** What GitHub says about the issue; `null` when it could not be loaded. */
  code: IssueGitHub | null;
  /** Whether the project has a repository. */
  connected: boolean;
  members: Assignable[];
  me: number;
}) {
  const t = useTranslations('issues');
  const tags = aggregates?.tags ?? [];
  // A connected repository with nothing to say about this issue gets no
  // block; an unconnected one gets the line that offers to connect it.
  const showCode =
    !connected ||
    (!!code && (code.suspect_commits.length > 0 || code.owners.length > 0));

  return (
    <div className="flex flex-col">
      <RailBlock title={t('detail.properties')}>
        <IssueProperties
          issue={issue}
          projectId={projectId}
          members={members}
          me={me}
          userCount={aggregates?.user_count ?? 0}
        />
      </RailBlock>

      {tags.length > 0 ? (
        <RailBlock title={t('detail.tags')}>
          <TagDistribution tags={tags.slice(0, 6)} />
        </RailBlock>
      ) : null}

      {showCode ? (
        <RailBlock title={t('detail.code')}>
          <IssueCodeBlock
            projectId={projectId}
            code={code}
            connected={connected ?? false}
          />
        </RailBlock>
      ) : null}

      <RailBlock title={t('detail.activity')}>
        <IssueActivity
          projectId={projectId}
          issueId={issueId}
          activity={activity}
        />
      </RailBlock>
    </div>
  );
}
