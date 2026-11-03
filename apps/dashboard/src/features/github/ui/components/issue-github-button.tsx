import type {
  CreateGitHubIssue,
  GitHubIssueDraft,
  IssueLink,
} from '@rustrak/client';
import {
  Button,
  Dialog,
  DialogBody,
  DialogFooter,
  DialogHeader,
  Field,
  FieldLabel,
  Input,
  LinkIcon,
  Menu,
  type MenuAction,
  NewIcon,
  Spinner,
  Textarea,
  UnlinkIcon,
} from '@rustrak/ui';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { githubMutations } from '@/features/github/api/mutations';
import { githubQueries } from '@/features/github/api/queries';
import { parseLabels } from '@/features/github/lib/labels';
import { cn } from '@/shared/lib/utils';
import { GitHubMark } from '@/shared/ui/components/github-mark';

interface IssueGitHubButtonProps {
  projectId: number;
  issueId: string;
  links: IssueLink[];
  /** False once the project's repository is gone: links stay, new ones can't. */
  canLink?: boolean;
}

/**
 * The issue's way to GitHub, in the toolbar beside resolve and archive. Each
 * linked issue is a chip that opens it; the menu creates or links another.
 */
export function IssueGitHubButton({
  projectId,
  issueId,
  links,
  canLink = true,
}: IssueGitHubButtonProps) {
  const t = useTranslations('github');
  const [linking, setLinking] = useState(false);
  const [creating, setCreating] = useState(false);
  const [reference, setReference] = useState('');
  const failed = (failure: string) => (error: Error) =>
    toast.error(failure, { description: error.message });
  const create = useMutation({
    ...githubMutations.createIssue(projectId, issueId),
    onError: failed(t('issue.createFailed')),
  });
  const linkIssue = useMutation({
    ...githubMutations.linkIssue(projectId, issueId),
    onError: failed(t('issue.linkFailed')),
  });
  const unlink = useMutation({
    ...githubMutations.unlinkIssue(projectId, issueId),
    onError: failed(t('issue.unlinkFailed')),
  });
  const isPending = create.isPending || linkIssue.isPending || unlink.isPending;

  const actions: MenuAction[] = [
    ...(canLink
      ? [
          {
            id: 'create',
            label: t('issue.create'),
            icon: NewIcon,
            onSelect: () => setCreating(true),
          },
          {
            id: 'link',
            label: t('issue.link'),
            icon: LinkIcon,
            onSelect: () => setLinking(true),
          },
        ]
      : []),
    ...links.map((link, index) => ({
      id: `unlink-${link.id}`,
      label: t('issue.unlink', { number: link.number }),
      icon: UnlinkIcon,
      separated: canLink && index === 0,
      onSelect: () => unlink.mutate(link.id),
    })),
  ];

  return (
    <div className="flex items-center gap-1.5">
      {links.map((link) => (
        <Button
          key={link.id}
          size="sm"
          render={
            <a
              href={link.url}
              target="_blank"
              rel="noreferrer"
              title={`${link.repo_full_name}#${link.number} · ${link.title} · ${
                link.state === 'closed' ? t('issue.closed') : t('issue.open')
              }`}
            />
          }
        >
          <GitHubMark className="size-3.5" />#{link.number}
          <span
            className={cn(
              'size-1.5 rounded-full',
              link.state === 'closed' ? 'bg-fg-subtle' : 'bg-success',
            )}
          />
        </Button>
      ))}

      <Menu
        trigger={
          <Button
            size="sm"
            menu
            loading={isPending}
            aria-label={t('issue.menu')}
          >
            {links.length === 0 ? (
              <>
                {!isPending && <GitHubMark className="size-3.5" />}
                GitHub
              </>
            ) : undefined}
          </Button>
        }
        actions={actions}
      />

      <Dialog open={creating} onOpenChange={setCreating} size="lg">
        <DialogHeader
          title={t('issue.createTitle')}
          description={t('issue.createDescription')}
        />
        {creating && (
          <CreateIssueDraft
            projectId={projectId}
            issueId={issueId}
            isPending={isPending}
            onSubmit={(input) =>
              create.mutate(input, { onSuccess: () => setCreating(false) })
            }
          />
        )}
      </Dialog>

      <Dialog open={linking} onOpenChange={setLinking} size="md">
        <form
          className="flex min-h-0 flex-col"
          onSubmit={(e) => {
            e.preventDefault();
            linkIssue.mutate(reference.trim(), {
              onSuccess: () => {
                setLinking(false);
                setReference('');
              },
            });
          }}
        >
          <DialogHeader
            title={t('issue.linkTitle')}
            description={t('issue.linkDescription')}
          />
          <DialogBody>
            <Input
              autoFocus
              value={reference}
              onChange={(e) => setReference(e.target.value)}
              placeholder="#123"
              aria-label={t('issue.linkTitle')}
            />
          </DialogBody>
          <DialogFooter>
            <Button
              type="submit"
              variant="primary"
              disabled={isPending || !reference.trim()}
            >
              {t('issue.linkSubmit')}
            </Button>
          </DialogFooter>
        </form>
      </Dialog>
    </div>
  );
}

/** Reads the draft the server would send, then lets the reader edit it. */
function CreateIssueDraft({
  projectId,
  issueId,
  isPending,
  onSubmit,
}: {
  projectId: number;
  issueId: string;
  isPending: boolean;
  onSubmit: (input: CreateGitHubIssue) => void;
}) {
  const draft = useQuery(githubQueries.draft(projectId, issueId));
  if (draft.isError) {
    return (
      <DialogBody>
        <p className="text-danger-fg text-meta">{draft.error.message}</p>
      </DialogBody>
    );
  }
  if (!draft.data) {
    return (
      <DialogBody className="flex justify-center">
        <Spinner />
      </DialogBody>
    );
  }
  return (
    <CreateIssueForm
      draft={draft.data}
      isPending={isPending}
      onSubmit={onSubmit}
    />
  );
}

function CreateIssueForm({
  draft,
  isPending,
  onSubmit,
}: {
  draft: GitHubIssueDraft;
  isPending: boolean;
  onSubmit: (input: CreateGitHubIssue) => void;
}) {
  const t = useTranslations('github');
  // Starts from the draft and is the reader's from then on.
  // react-doctor-disable-next-line react-doctor/no-derived-useState
  const [title, setTitle] = useState(draft.title);
  // react-doctor-disable-next-line react-doctor/no-derived-useState
  const [body, setBody] = useState(draft.body);
  // react-doctor-disable-next-line react-doctor/no-derived-useState
  const [labels, setLabels] = useState(() => draft.labels.join(', '));
  // react-doctor-disable-next-line react-doctor/no-derived-useState
  const [assignee, setAssignee] = useState(draft.assignee ?? '');
  return (
    <form
      className="flex min-h-0 flex-col"
      onSubmit={(e) => {
        e.preventDefault();
        onSubmit({
          title,
          body,
          labels: parseLabels(labels),
          assignee: assignee.trim(),
        });
      }}
    >
      <DialogBody className="flex flex-col gap-4">
        <Field>
          <FieldLabel>{t('issue.titleField')}</FieldLabel>
          <Input
            autoFocus
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
        </Field>
        <Field>
          <FieldLabel>{t('issue.bodyField')}</FieldLabel>
          <Textarea
            className="max-h-80 font-mono text-code"
            value={body}
            onChange={(e) => setBody(e.target.value)}
          />
        </Field>
        <Field>
          <FieldLabel>{t('issue.labelsField')}</FieldLabel>
          <Input
            value={labels}
            placeholder="bug, rustrak"
            onChange={(e) => setLabels(e.target.value)}
          />
        </Field>
        <Field>
          <FieldLabel>{t('issue.assigneeField')}</FieldLabel>
          {/* Free text with GitHub's choices as suggestions: a repository
              GitHub could not list still takes a login. */}
          <Input
            list="github-issue-assignees"
            value={assignee}
            placeholder="octocat"
            onChange={(e) => setAssignee(e.target.value)}
          />
          <datalist id="github-issue-assignees">
            {(draft.assignees ?? []).map((login) => (
              <option key={login} value={login} />
            ))}
          </datalist>
        </Field>
      </DialogBody>
      <DialogFooter>
        <Button
          type="submit"
          variant="primary"
          disabled={isPending || !title.trim()}
        >
          {t('issue.createSubmit')}
        </Button>
      </DialogFooter>
    </form>
  );
}
