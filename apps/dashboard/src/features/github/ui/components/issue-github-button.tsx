import type {
  CreateGitHubIssue,
  GitHubIssueDraft,
  IssueLink,
} from '@rustrak/client';
import { useMutation, useQuery } from '@tanstack/react-query';
import { ChevronDown, Link2, Loader2, Plus, Unlink } from 'lucide-react';
import { useState } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { githubMutations } from '@/features/github/api/mutations';
import { githubQueries } from '@/features/github/api/queries';
import { parseLabels } from '@/features/github/lib/labels';
import { cn } from '@/shared/lib/utils';
import { GitHubMark } from '@/shared/ui/components/github-mark';
import { Button, buttonVariants } from '@/shared/ui/components/shadcn/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/shared/ui/components/shadcn/dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/shared/ui/components/shadcn/dropdown-menu';
import { Input } from '@/shared/ui/components/shadcn/input';
import { Label } from '@/shared/ui/components/shadcn/label';
import { Textarea } from '@/shared/ui/components/shadcn/textarea';

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

  return (
    <div className="flex items-center gap-1.5">
      {links.map((link) => (
        <a
          key={link.id}
          href={link.url}
          target="_blank"
          rel="noreferrer"
          title={`${link.repo_full_name}#${link.number} · ${link.title} · ${
            link.state === 'closed' ? t('issue.closed') : t('issue.open')
          }`}
          className={cn(buttonVariants({ variant: 'outline', size: 'sm' }))}
        >
          <GitHubMark className="size-3.5" />#{link.number}
          <span
            className={cn(
              'size-1.5 rounded-full',
              link.state === 'closed' ? 'bg-purple-500' : 'bg-green-500',
            )}
          />
        </a>
      ))}

      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              variant="outline"
              size="sm"
              disabled={isPending}
              aria-label={t('issue.menu')}
            />
          }
        >
          {isPending ? (
            <Loader2 className="size-4 animate-spin" />
          ) : (
            links.length === 0 && <GitHubMark className="size-3.5" />
          )}
          {links.length === 0 && 'GitHub'}
          <ChevronDown className="size-3.5" />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-56">
          {canLink && (
            <>
              <DropdownMenuItem onClick={() => setCreating(true)}>
                <Plus className="mr-2 size-4" />
                {t('issue.create')}
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => setLinking(true)}>
                <Link2 className="mr-2 size-4" />
                {t('issue.link')}
              </DropdownMenuItem>
            </>
          )}
          {canLink && links.length > 0 && <DropdownMenuSeparator />}
          {links.map((link) => (
            <DropdownMenuItem
              key={link.id}
              onClick={() => unlink.mutate(link.id)}
            >
              <Unlink className="mr-2 size-4" />
              {t('issue.unlink', { number: link.number })}
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>

      <Dialog open={creating} onOpenChange={setCreating}>
        <DialogContent className="sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>{t('issue.createTitle')}</DialogTitle>
            <DialogDescription>
              {t('issue.createDescription')}
            </DialogDescription>
          </DialogHeader>
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
        </DialogContent>
      </Dialog>

      <Dialog open={linking} onOpenChange={setLinking}>
        <DialogContent className="sm:max-w-md">
          <form
            className="space-y-4"
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
            <DialogHeader>
              <DialogTitle>{t('issue.linkTitle')}</DialogTitle>
              <DialogDescription>
                {t('issue.linkDescription')}
              </DialogDescription>
            </DialogHeader>
            <Input
              autoFocus
              value={reference}
              onChange={(e) => setReference(e.target.value)}
              placeholder="#123"
              aria-label={t('issue.linkTitle')}
            />
            <DialogFooter>
              <Button type="submit" disabled={isPending || !reference.trim()}>
                {t('issue.linkSubmit')}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
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
    return <p className="text-sm text-destructive">{draft.error.message}</p>;
  }
  if (!draft.data) {
    return <Loader2 className="mx-auto size-5 animate-spin" />;
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
      className="space-y-4"
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
      <div className="space-y-1.5">
        <Label htmlFor="github-issue-title">{t('issue.titleField')}</Label>
        <Input
          id="github-issue-title"
          autoFocus
          value={title}
          onChange={(e) => setTitle(e.target.value)}
        />
      </div>
      <div className="space-y-1.5">
        <Label htmlFor="github-issue-body">{t('issue.bodyField')}</Label>
        <Textarea
          id="github-issue-body"
          className="max-h-80 font-mono text-xs"
          value={body}
          onChange={(e) => setBody(e.target.value)}
        />
      </div>
      <div className="space-y-1.5">
        <Label htmlFor="github-issue-labels">{t('issue.labelsField')}</Label>
        <Input
          id="github-issue-labels"
          value={labels}
          placeholder="bug, rustrak"
          onChange={(e) => setLabels(e.target.value)}
        />
      </div>
      <div className="space-y-1.5">
        <Label htmlFor="github-issue-assignee">
          {t('issue.assigneeField')}
        </Label>
        {/* Free text with GitHub's choices as suggestions: a repository
            GitHub could not list still takes a login. */}
        <Input
          id="github-issue-assignee"
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
      </div>
      <DialogFooter>
        <Button type="submit" disabled={isPending || !title.trim()}>
          {t('issue.createSubmit')}
        </Button>
      </DialogFooter>
    </form>
  );
}
