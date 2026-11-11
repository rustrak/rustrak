import type { ProjectResponse } from '@rustrak/client';
import { Button, confirm, DeleteIcon } from '@rustrak/ui';
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
  isPending,
}: {
  project: ProjectResponse;
  onRemove: () => void;
  /** The removal is in flight; not the form's save, which is unrelated. */
  isPending: boolean;
}) {
  const t = useTranslations('projects');

  const remove = async () => {
    const ok = await confirm({
      title: t('dangerZone.deleteTitle', { name: project.name }),
      description: t('dangerZone.deleteDescription'),
      tone: 'danger',
      confirmLabel: t('dangerZone.confirm'),
      cancelLabel: t('dangerZone.cancel'),
    });
    if (ok) onRemove();
  };

  return (
    <SettingSection title={t('dangerZone.title')} destructive>
      <SettingRow
        title={t('dangerZone.remove')}
        description={t('dangerZone.removeDescription')}
      >
        <Button
          variant="danger"
          icon={DeleteIcon}
          onClick={remove}
          loading={isPending}
        >
          {t('dangerZone.remove')}
        </Button>
      </SettingRow>
    </SettingSection>
  );
}
