import { Button, Dialog, DialogFooter, DialogHeader } from '@rustrak/ui';
import { useTranslations } from 'use-intl';

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
    <Dialog alert open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogHeader
        title={title}
        description={description}
        dismissible={false}
      />
      <DialogFooter>
        <Button variant="secondary" disabled={isPending} onClick={onClose}>
          {t('common.cancel')}
        </Button>
        <Button
          variant="danger-primary"
          loading={isPending}
          onClick={onConfirm}
        >
          {isPending ? t('common.deleting') : t('common.delete')}
        </Button>
      </DialogFooter>
    </Dialog>
  );
}
