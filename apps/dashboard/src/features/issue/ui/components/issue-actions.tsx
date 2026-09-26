import type {
  Issue,
  IssuePriority,
  Result,
  RustrakError,
} from '@rustrak/client';
import { useNavigate } from '@tanstack/react-router';
import {
  Archive,
  ArchiveRestore,
  Bell,
  BellOff,
  Bookmark,
  BookmarkCheck,
  Check,
  ChevronDown,
  Loader2,
  MoreHorizontal,
  Rocket,
  Trash2,
  Undo,
} from 'lucide-react';
import { useState, useTransition } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import {
  deleteIssue,
  resolveIssueInNextRelease,
  setIssueBookmark,
  setIssueSubscription,
  updateIssueState,
} from '@/features/issue/api/mutations';
import { invalidateIssues } from '@/features/issue/api/queries';
import { priorityDisplay } from '@/features/issue/model/status';
import { cn } from '@/shared/lib/utils';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/shared/ui/components/shadcn/alert-dialog';
import { Button } from '@/shared/ui/components/shadcn/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/shared/ui/components/shadcn/dropdown-menu';

interface IssueActionsProps {
  issue: Issue;
  projectId: number;
}

const PRIORITIES: IssuePriority[] = ['high', 'medium', 'low'];

export function IssueActions({ issue, projectId }: IssueActionsProps) {
  const t = useTranslations('issues');
  const navigate = useNavigate();
  const [, startTransition] = useTransition();
  const [pending, setPending] = useState<string | null>(null);
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);

  const busy = pending !== null;
  const isResolved = issue.status === 'resolved';
  const isArchived = issue.status === 'ignored';
  const priorityMeta = priorityDisplay(issue.priority);

  /**
   * Run one toolbar action and report whether it worked.
   *
   * The `try`/`finally` this replaces had no `catch`, which used to be fine
   * because a throw propagated. Now that the actions return their failure, an
   * ignored `Result` means a resolve or a mute that silently did nothing: the
   * button stops spinning, the refresh puts the old state back, and nothing
   * says why.
   */
  const run = (
    action: string,
    label: string,
    fn: () => Promise<Result<unknown, RustrakError>>,
  ) => {
    setPending(action);
    startTransition(async () => {
      const result = await fn();

      if (!result.success) {
        toast.error(label, { description: result.error.message });
      } else {
        void invalidateIssues(projectId);
      }

      setPending(null);
    });
  };

  const handleConfirmDelete = () => {
    setPending('delete');
    startTransition(async () => {
      const result = await deleteIssue(projectId, issue.id);

      if (!result.success) {
        toast.error(t('toasts.deleteOneFailed'), {
          description: result.error.message,
        });
        setPending(null);
        return;
      }

      setDeleteDialogOpen(false);
      navigate({ to: '/projects/$id', params: { id: projectId } });
      setPending(null);
    });
  };

  return (
    <div className="flex items-center justify-between gap-x-3 gap-y-2 flex-wrap">
      {/* Left cluster */}
      <div className="flex items-center gap-2 flex-wrap">
        {/* Resolve (split) */}
        <div className="flex">
          <Button
            size="sm"
            className={cn(!isResolved && 'rounded-r-none')}
            variant={isResolved ? 'outline' : 'default'}
            disabled={busy}
            onClick={() =>
              run('resolve', t('toasts.updateFailed'), () =>
                updateIssueState(projectId, issue.id, {
                  status: isResolved ? 'unresolved' : 'resolved',
                }),
              )
            }
          >
            {isResolved ? (
              <Undo className="mr-2 size-4" />
            ) : (
              <Check className="mr-2 size-4" />
            )}
            {isResolved ? t('actions.unresolve') : t('actions.resolve')}
          </Button>
          {!isResolved && (
            <DropdownMenu>
              <DropdownMenuTrigger
                render={
                  <Button
                    size="sm"
                    className="rounded-l-none border-l border-primary-foreground/20 px-2"
                    aria-label={t('moreResolveOptions')}
                  />
                }
              >
                <ChevronDown className="size-4" />
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start">
                <DropdownMenuItem
                  disabled={busy}
                  onClick={() =>
                    run('next-release', t('toasts.updateFailed'), () =>
                      resolveIssueInNextRelease(projectId, issue.id),
                    )
                  }
                >
                  <Rocket className="mr-2 size-4" />
                  {t('actions.resolveInNextRelease')}
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          )}
        </div>

        {/* Archive (mute) */}
        <Button
          variant="outline"
          size="sm"
          disabled={busy}
          onClick={() =>
            run('archive', t('toasts.updateFailed'), () =>
              updateIssueState(projectId, issue.id, {
                status: isArchived ? 'unresolved' : 'ignored',
              }),
            )
          }
        >
          {isArchived ? (
            <ArchiveRestore className="mr-2 size-4" />
          ) : (
            <Archive className="mr-2 size-4" />
          )}
          {isArchived ? t('actions.unarchive') : t('actions.archive')}
        </Button>

        {/* Overflow */}
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <Button
                variant="outline"
                size="sm"
                className="size-8 px-0"
                aria-label={t('issueActionsLabel')}
              />
            }
          >
            <MoreHorizontal className="size-4" />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-52">
            <DropdownMenuItem
              disabled={busy}
              onClick={() =>
                run('bookmark', t('toasts.bookmarkFailed'), () =>
                  setIssueBookmark(projectId, issue.id, !issue.is_bookmarked),
                )
              }
            >
              {issue.is_bookmarked ? (
                <BookmarkCheck className="mr-2 size-4" />
              ) : (
                <Bookmark className="mr-2 size-4" />
              )}
              {issue.is_bookmarked
                ? t('actions.removeBookmark')
                : t('actions.bookmark')}
            </DropdownMenuItem>
            <DropdownMenuItem
              disabled={busy}
              onClick={() =>
                run('subscribe', t('toasts.subscriptionFailed'), () =>
                  setIssueSubscription(
                    projectId,
                    issue.id,
                    !issue.is_subscribed,
                  ),
                )
              }
            >
              {issue.is_subscribed ? (
                <BellOff className="mr-2 size-4" />
              ) : (
                <Bell className="mr-2 size-4" />
              )}
              {issue.is_subscribed
                ? t('actions.unsubscribe')
                : t('actions.subscribe')}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              variant="destructive"
              disabled={busy}
              onClick={() => setDeleteDialogOpen(true)}
            >
              <Trash2 className="mr-2 size-4" />
              {t('actions.deleteIssue')}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      {/* Right: priority */}
      <div className="flex items-center gap-2">
        <span className="text-sm text-muted-foreground">{t('priority')}</span>
        <DropdownMenu>
          <DropdownMenuTrigger
            render={<Button variant="outline" size="sm" disabled={busy} />}
          >
            {priorityMeta ? (
              <span className="inline-flex items-center gap-1.5">
                <span
                  className={cn('size-1.5 rounded-full', priorityMeta.dot)}
                />
                {t(priorityMeta.labelKey)}
              </span>
            ) : (
              <span className="text-muted-foreground">{t('set')}</span>
            )}
            <ChevronDown className="ml-1.5 size-3.5" />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {PRIORITIES.map((priority) => {
              const meta = priorityDisplay(priority);
              return (
                <DropdownMenuItem
                  key={priority}
                  disabled={busy}
                  onClick={() =>
                    run('priority', t('toasts.priorityFailed'), () =>
                      updateIssueState(projectId, issue.id, { priority }),
                    )
                  }
                >
                  <span
                    className={cn(
                      'mr-2 size-1.5 rounded-full',
                      meta?.dot ?? 'bg-muted-foreground',
                    )}
                  />
                  {meta ? t(meta.labelKey) : priority}
                  {issue.priority === priority && (
                    <Check className="ml-auto size-4 text-muted-foreground" />
                  )}
                </DropdownMenuItem>
              );
            })}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      {/* Delete confirmation */}
      <AlertDialog open={deleteDialogOpen} onOpenChange={setDeleteDialogOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t('deleteDialog.title', { count: 1 })}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t('deleteDialog.description', { count: 1 })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={pending === 'delete'}>
              {t('deleteDialog.cancel')}
            </AlertDialogCancel>
            <AlertDialogAction
              onClick={handleConfirmDelete}
              disabled={pending === 'delete'}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              {pending === 'delete' ? (
                <>
                  <Loader2 className="mr-2 size-4 animate-spin" />
                  {t('deleteDialog.deleting')}
                </>
              ) : (
                t('deleteDialog.confirm')
              )}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
