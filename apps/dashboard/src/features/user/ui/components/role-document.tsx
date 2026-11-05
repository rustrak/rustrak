import type { CatalogResource, Permission, Role } from '@rustrak/client';
import { Button, confirm, DeleteIcon, Input, Textarea } from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { type ReactNode, useState } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { userMutations } from '@/features/user/api/mutations';
import { grantable } from '@/shared/lib/permission-matrix';
import { PermissionEditor } from '@/shared/ui/components/permission-editor';
import { SettingRow, SettingSection } from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
import { usePermissionsLater } from '@/shared/ui/hooks/use-permissions';
import { useRouter } from '@/shared/ui/hooks/use-router';

/**
 * One role: its name and description, then one tab per module with its
 * permissions as a matrix. Edits stay in a draft across tabs until Save.
 */
export function RoleDocument({
  role,
  catalog,
  canEdit,
  canDelete,
}: {
  role: Role;
  catalog: CatalogResource[];
  /** `roles:update`. */
  canEdit: boolean;
  /** `roles:delete`. */
  canDelete: boolean;
}) {
  const t = useTranslations('access');
  const router = useRouter();
  const reader = usePermissionsLater();
  // The callbacks live here, not on `mutate`: a saved or deleted role remounts
  // or leaves this page, and `mutate`'s own callbacks do not run after that.
  const save = useMutation({
    ...userMutations.updateRole(role.id),
    onSuccess: () => toast.success(t('roles.saved')),
    // "You cannot grant permissions you do not hold" says exactly why.
    onError: (error) =>
      toast.error(t('roles.saveFailed'), {
        description: error.error.message,
      }),
  });
  const remove = useMutation({
    ...userMutations.deleteRole(),
    onSuccess: () => {
      toast.success(t('roles.deleted'));
      router.push('/settings/roles');
    },
    // A role still held comes back as a conflict saying how often.
    onError: (error) =>
      toast.error(t('roles.deleteFailed'), {
        description: error.error.message,
      }),
  });
  // A draft of the role, seeded once: the page remounts this with a new
  // `key` when the saved role changes, so it never goes stale.
  const [name, setName] = useState(() => role.name);
  const [description, setDescription] = useState(() => role.description);
  const [permissions, setPermissions] = useState<Permission[]>(
    () => role.permissions,
  );

  const dirty =
    name !== role.name ||
    description !== role.description ||
    JSON.stringify(permissions) !== JSON.stringify(role.permissions);

  const submit = () =>
    save.mutate({ name: name.trim(), description, permissions });

  const destroy = async () => {
    const ok = await confirm({
      title: t('roles.deleteTitle', { name: role.name }),
      description: t('roles.deleteDescription'),
      tone: 'danger',
      confirmLabel: t('roles.delete'),
      cancelLabel: t('cancel'),
    });
    if (!ok) return;
    remove.mutate(role.id);
  };

  const header = (tabs: ReactNode) => (
    <SettingsHeader
      tabs={tabs}
      title={role.name}
      description={role.description || undefined}
      actions={
        <>
          {canDelete ? (
            <Button
              variant="danger"
              icon={DeleteIcon}
              onClick={destroy}
              loading={remove.isPending}
            >
              {t('roles.delete')}
            </Button>
          ) : null}
          {canEdit ? (
            <Button
              variant="primary"
              onClick={submit}
              loading={save.isPending}
              disabled={!dirty || !name.trim()}
            >
              {t('save')}
            </Button>
          ) : null}
        </>
      }
    />
  );

  const general = (
    <div className="flex flex-col gap-6">
      <SettingSection
        title={t('roles.detailsTitle')}
        description={t('roles.assignedTo', { count: role.assignment_count })}
      >
        <SettingRow title={t('roles.name')}>
          <Input
            value={name}
            disabled={!canEdit}
            onChange={(e) => setName(e.target.value)}
          />
        </SettingRow>
        <SettingRow
          title={t('roles.descriptionLabel')}
          description={t('roles.descriptionHint')}
          wide
        >
          <Textarea
            value={description}
            rows={3}
            disabled={!canEdit}
            placeholder={t('roles.descriptionPlaceholder')}
            onChange={(e) => setDescription(e.target.value)}
          />
        </SettingRow>
      </SettingSection>
    </div>
  );

  return (
    <PermissionEditor
      header={header}
      catalog={canEdit ? grantable(catalog, reader) : catalog}
      value={permissions}
      onChange={setPermissions}
      readOnly={!canEdit}
      leading={[{ value: 'general', label: t('general'), content: general }]}
    />
  );
}
