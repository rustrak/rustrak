import type { Monitor } from '@rustrak/client';
import { Bell, BellOff, MoreHorizontal, Pencil, Trash2 } from 'lucide-react';
import { useState, useTransition } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { deleteMonitor, updateMonitor } from '@/features/monitor/api/mutations';
import { Link } from '@/shared/ui/components/link';
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
import { useRouter } from '@/shared/ui/hooks/use-router';

/** Edit, and behind the menu, mute and delete. */
export function MonitorActions({
  projectId,
  monitor,
}: {
  projectId: number;
  monitor: Monitor;
}) {
  const t = useTranslations('crons');
  const router = useRouter();
  const [isPending, startTransition] = useTransition();
  const [confirming, setConfirming] = useState(false);
  const base = `/projects/${projectId}/crons`;
  const href = `${base}/${encodeURIComponent(monitor.slug)}`;

  const toggleMute = () =>
    startTransition(async () => {
      const result = await updateMonitor(projectId, monitor.slug, {
        is_muted: !monitor.is_muted,
      });
      if (!result.success) {
        toast.error(t('actions.updateFailed'));
        return;
      }
      toast.success(t(monitor.is_muted ? 'actions.unmuted' : 'actions.muted'));
      await router.refresh();
    });

  const remove = () =>
    startTransition(async () => {
      const result = await deleteMonitor(projectId, monitor.slug);
      if (!result.success) {
        toast.error(t('actions.deleteFailed'));
        return;
      }
      toast.success(t('actions.deleted'));
      setConfirming(false);
      router.push(base);
    });

  return (
    <div className="flex items-center gap-2">
      <Button
        variant="outline"
        size="sm"
        nativeButton={false}
        render={<Link href={`${href}/edit`} />}
      >
        <Pencil />
        {t('actions.edit')}
      </Button>
      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              variant="outline"
              size="icon-sm"
              aria-label={t('actions.more')}
              disabled={isPending}
            >
              <MoreHorizontal />
            </Button>
          }
        />
        <DropdownMenuContent align="end" className="w-44">
          <DropdownMenuItem onClick={toggleMute}>
            {monitor.is_muted ? <Bell /> : <BellOff />}
            {t(monitor.is_muted ? 'actions.unmute' : 'actions.mute')}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem
            variant="destructive"
            onClick={() => setConfirming(true)}
          >
            <Trash2 />
            {t('actions.delete')}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      <AlertDialog
        open={confirming}
        onOpenChange={(open) => !open && !isPending && setConfirming(false)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t('actions.deleteTitle', { monitor: monitor.name })}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t('actions.deleteDescription')}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={isPending}>
              {t('actions.cancel')}
            </AlertDialogCancel>
            <AlertDialogAction
              onClick={remove}
              disabled={isPending}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              {isPending ? t('actions.deleting') : t('actions.confirmDelete')}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
