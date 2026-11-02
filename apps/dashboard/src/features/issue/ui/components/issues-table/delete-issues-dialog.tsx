import { Button, Dialog, DialogFooter, DialogHeader } from '@rustrak/ui';
import { useTranslations } from 'use-intl';

/**
 * The confirmation for deleting one issue or a ticked batch.
 *
 * One dialog for both because the question is the same and only the count
 * changes: `count` of 1 is the single-issue case, so the caller never has to
 * say which mode it is in.
 */
export function DeleteIssuesDialog({
  open,
  onOpenChange,
  count,
  isPending,
  onConfirm,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  count: number;
  isPending: boolean;
  onConfirm: () => void;
}) {
  const t = useTranslations('issues');

  return (
    <Dialog alert open={open} onOpenChange={onOpenChange}>
      <DialogHeader
        title={t('deleteDialog.title', { count })}
        description={t('deleteDialog.description', { count })}
        dismissible={false}
      />
      <DialogFooter>
        <Button
          variant="secondary"
          disabled={isPending}
          onClick={() => onOpenChange(false)}
        >
          {t('deleteDialog.cancel')}
        </Button>
        <Button
          variant="danger-primary"
          loading={isPending}
          onClick={onConfirm}
        >
          {isPending ? t('deleteDialog.deleting') : t('deleteDialog.confirm')}
        </Button>
      </DialogFooter>
    </Dialog>
  );
}
