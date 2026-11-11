import type { MonitorResponse } from '@rustrak/client';
import {
  Button,
  confirm,
  DeleteIcon,
  EditIcon,
  Menu,
  MoreIcon,
  MuteIcon,
  NotificationIcon,
} from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { monitorMutations } from '@/features/monitor/api/mutations';
import { editMonitorHref, monitorsHref } from '@/features/monitor/lib/href';
import type { ApiError } from '@/shared/api/query';
import { Link } from '@/shared/ui/components/link';
import { usePermissionsLater } from '@/shared/ui/hooks/use-permissions';
import { useRouter } from '@/shared/ui/hooks/use-router';
import { useToastFailure } from '@/shared/ui/hooks/use-toast-failure';

/**
 * Mute, the verb a failing monitor gets most, then edit, and delete behind
 * the menu with a confirmation.
 */
export function MonitorActions({
  projectId,
  monitor,
}: {
  projectId: number;
  monitor: MonitorResponse;
}) {
  const t = useTranslations('crons');
  const toastFailure = useToastFailure();
  const router = useRouter();
  const update = useMutation(monitorMutations.update(projectId));
  const del = useMutation(monitorMutations.delete(projectId, monitor.slug));
  const isPending = update.isPending || del.isPending;
  const permissions = usePermissionsLater();
  const canUpdate = permissions.can('monitors', 'update', projectId);
  const canDelete = permissions.can('monitors', 'delete', projectId);

  const toggleMute = () =>
    update.mutate(
      { slug: monitor.slug, body: { is_muted: !monitor.is_muted } },
      {
        onError: (error) => toastFailure(t('actions.updateFailed'), error),
        onSuccess: () =>
          toast.success(
            t(monitor.is_muted ? 'actions.unmuted' : 'actions.muted'),
          ),
      },
    );

  const remove = async () => {
    const ok = await confirm({
      title: t('actions.deleteTitle', { monitor: monitor.name }),
      description: t('actions.deleteDescription'),
      tone: 'danger',
      confirmLabel: t('actions.confirmDelete'),
      cancelLabel: t('actions.cancel'),
    });
    if (!ok) return;
    // Leave first: this page re-reading a deleted monitor can only 404, and
    // unmounting would drop `mutate`'s callbacks with it.
    router.push(monitorsHref(projectId));
    await del.mutateAsync().then(
      () => toast.success(t('actions.deleted')),
      (error: ApiError) => toastFailure(t('actions.deleteFailed'), error),
    );
  };

  return (
    <div className="flex items-center gap-2">
      {canUpdate ? (
        <>
          <Button
            icon={monitor.is_muted ? NotificationIcon : MuteIcon}
            disabled={isPending}
            onClick={toggleMute}
          >
            {t(monitor.is_muted ? 'actions.unmute' : 'actions.mute')}
          </Button>
          <Button
            icon={EditIcon}
            render={<Link href={editMonitorHref(projectId, monitor.slug)} />}
          >
            {t('actions.edit')}
          </Button>
        </>
      ) : null}
      {canDelete ? (
        <Menu
          align="end"
          trigger={
            <Button
              variant="ghost"
              icon={MoreIcon}
              aria-label={t('actions.more')}
              disabled={isPending}
            />
          }
          actions={[
            {
              id: 'delete',
              label: t('actions.delete'),
              icon: DeleteIcon,
              tone: 'danger',
              onSelect: remove,
            },
          ]}
        />
      ) : null}
    </div>
  );
}
