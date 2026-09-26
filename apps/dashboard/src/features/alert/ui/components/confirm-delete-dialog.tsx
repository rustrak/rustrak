import { useTranslations } from 'use-intl';
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

/**
 * The confirmation before an alert rule or an integration is deleted.
 *
 * Both buttons lock while the delete is in flight, so a second click cannot
 * fire a second request and the dialog cannot be dismissed half-way.
 */
export function ConfirmDeleteDialog({
  open,
  title,
  description,
  isPending,
  onConfirm,
  onClose,
}: {
  open: boolean;
  title: string;
  description: string;
  isPending: boolean;
  onConfirm: () => void;
  onClose: () => void;
}) {
  const t = useTranslations('alerts');

  return (
    <AlertDialog open={open} onOpenChange={(next) => !next && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          <AlertDialogDescription>{description}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={isPending}>
            {t('common.cancel')}
          </AlertDialogCancel>
          <AlertDialogAction
            onClick={onConfirm}
            disabled={isPending}
            className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
          >
            {isPending ? t('common.deleting') : t('common.delete')}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
