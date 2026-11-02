import {
  Button,
  Dialog,
  DialogClose,
  DialogFooter,
  DialogHeader,
} from '@rustrak/ui';
import { useTranslations } from 'use-intl';

/**
 * The confirmation for deleting one project or a ticked batch.
 *
 * `targetName` is what distinguishes the two: naming the project is worth a
 * lot on a destructive action, and a batch has no single name to give.
 */
export function DeleteProjectsDialog({
  open,
  onOpenChange,
  count,
  targetName,
  isPending,
  onConfirm,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  count: number;
  targetName?: string;
  isPending: boolean;
  onConfirm: () => void;
}) {
  const t = useTranslations('projects');
  // A named target is always exactly one, whatever `count` says: the dialog is
  // then confirming that project rather than a selection.
  const subject = t('deleteDialog.subject', { count: targetName ? 1 : count });

  return (
    <Dialog alert open={open} onOpenChange={onOpenChange}>
      <DialogHeader
        dismissible={false}
        title={
          targetName
            ? t('deleteDialog.titleNamed', { name: targetName })
            : t('deleteDialog.titleCount', { count })
        }
        description={t('deleteDialog.description', { subject })}
      />
      <DialogFooter>
        <DialogClose
          disabled={isPending}
          render={
            <Button variant="secondary">{t('deleteDialog.cancel')}</Button>
          }
        />
        <Button
          variant="danger-primary"
          onClick={onConfirm}
          loading={isPending}
        >
          {isPending ? t('deleteDialog.deleting') : t('deleteDialog.confirm')}
        </Button>
      </DialogFooter>
    </Dialog>
  );
}
