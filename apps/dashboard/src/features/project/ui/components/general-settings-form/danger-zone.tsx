import type { ProjectResponse } from '@rustrak/client';
import {
  Button,
  DeleteIcon,
  Dialog,
  DialogClose,
  DialogFooter,
  DialogHeader,
} from '@rustrak/ui';
import { useState, useTransition } from 'react';
import { useTranslations } from 'use-intl';
import { SettingRow, SettingSection } from '@/shared/ui/components/setting-row';

/**
 * Removing the project, behind a confirmation that names it.
 *
 * Its own section rather than another row in the form above: everything there
 * is reversible by typing the old value back, and this is not.
 */
export function DangerZone({
  project,
  onRemove,
}: {
  project: ProjectResponse;
  /** Resolves once the request is done, whether it succeeded or not. */
  onRemove: () => Promise<void>;
}) {
  const [isOpen, setIsOpen] = useState(false);
  const t = useTranslations('projects');
  // Its own transition rather than the form's. Saving a name and removing the
  // project are unrelated requests, and sharing one `isPending` meant either
  // one in flight greyed out the other.
  const [isPending, startTransition] = useTransition();

  const remove = () =>
    startTransition(async () => {
      await onRemove();
      setIsOpen(false);
    });

  return (
    <>
      <SettingSection title={t('dangerZone.title')} destructive>
        <SettingRow
          title={t('dangerZone.remove')}
          description={t('dangerZone.removeDescription')}
        >
          <Button
            variant="danger"
            icon={DeleteIcon}
            onClick={() => setIsOpen(true)}
            disabled={isPending}
          >
            {t('dangerZone.remove')}
          </Button>
        </SettingRow>
      </SettingSection>

      <Dialog alert open={isOpen} onOpenChange={setIsOpen}>
        <DialogHeader
          dismissible={false}
          title={t('dangerZone.deleteTitle', { name: project.name })}
          description={t('dangerZone.deleteDescription')}
        />
        <DialogFooter>
          <DialogClose
            disabled={isPending}
            render={
              <Button variant="secondary">{t('dangerZone.cancel')}</Button>
            }
          />
          <Button variant="danger-primary" onClick={remove} loading={isPending}>
            {isPending ? t('dangerZone.deleting') : t('dangerZone.confirm')}
          </Button>
        </DialogFooter>
      </Dialog>
    </>
  );
}
