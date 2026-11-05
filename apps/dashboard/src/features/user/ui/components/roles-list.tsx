import type { Role } from '@rustrak/client';
import {
  Button,
  Dialog,
  DialogBody,
  DialogFooter,
  DialogHeader,
  Field,
  FieldLabel,
  Input,
  NewIcon,
  SecurityIcon,
  Select,
  SelectItem,
  TeamIcon,
} from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { userMutations } from '@/features/user/api/mutations';
import { Link } from '@/shared/ui/components/link';
import {
  SettingSection,
  SettingsEmpty,
  SettingsListItem,
} from '@/shared/ui/components/setting-row';
import { useRouter } from '@/shared/ui/hooks/use-router';

/** The roles there are, each row leading to its page. */
export function RolesList({
  roles,
  canCreate,
}: {
  roles: Role[];
  canCreate: boolean;
}) {
  const t = useTranslations('access');
  const [creating, setCreating] = useState(false);
  return (
    <SettingSection
      title={t('roles.title')}
      description={t('roles.description')}
      actions={
        canCreate ? (
          <Button icon={NewIcon} size="sm" onClick={() => setCreating(true)}>
            {t('roles.new')}
          </Button>
        ) : null
      }
    >
      {roles.length === 0 ? (
        <SettingsEmpty
          icon={<SecurityIcon size="2xl" />}
          title={t('roles.empty')}
        />
      ) : (
        roles.map((role) => (
          <SettingsListItem
            key={role.id}
            leading={
              <span className="flex size-8 items-center justify-center rounded-lg border border-border-subtle text-fg-muted">
                <SecurityIcon size="lg" />
              </span>
            }
            title={
              <Link
                href={`/settings/roles/${role.id}`}
                className="truncate hover:underline"
              >
                {role.name}
              </Link>
            }
            description={role.description || undefined}
            meta={
              <>
                <span>
                  {t('roles.permissionCount', {
                    count: role.permissions.length,
                  })}
                </span>
                <span className="flex items-center gap-1">
                  <TeamIcon size="sm" aria-hidden="true" />
                  {role.assignment_count}
                </span>
              </>
            }
          />
        ))
      )}
      <Dialog open={creating} onOpenChange={setCreating} size="sm">
        {creating ? (
          <NewRoleForm roles={roles} onDone={() => setCreating(false)} />
        ) : null}
      </Dialog>
    </SettingSection>
  );
}

const FROM_SCRATCH = 'none';

/** Name a role, optionally copying another's permissions; edit it next. */
function NewRoleForm({ roles, onDone }: { roles: Role[]; onDone: () => void }) {
  const t = useTranslations('access');
  const router = useRouter();
  const create = useMutation(userMutations.createRole());
  const [name, setName] = useState('');
  const [copyFrom, setCopyFrom] = useState(FROM_SCRATCH);

  const submit = () => {
    const source = roles.find((r) => String(r.id) === copyFrom);
    create.mutate(
      {
        name: name.trim(),
        description: '',
        permissions: source?.permissions ?? [],
      },
      {
        onSuccess: (role) => {
          onDone();
          router.push(`/settings/roles/${role.id}`);
        },
        onError: (error) =>
          toast.error(t('roles.createFailed'), {
            description: error.error.message,
          }),
      },
    );
  };

  return (
    <>
      <DialogHeader title={t('roles.newTitle')} />
      <DialogBody className="flex flex-col gap-4">
        <Field>
          <FieldLabel>{t('roles.name')}</FieldLabel>
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && name.trim() && submit()}
          />
        </Field>
        <Field>
          <FieldLabel>{t('roles.copyFrom')}</FieldLabel>
          <Select
            value={copyFrom}
            onValueChange={setCopyFrom}
            aria-label={t('roles.copyFrom')}
            items={{
              [FROM_SCRATCH]: t('roles.fromScratch'),
              ...Object.fromEntries(roles.map((r) => [String(r.id), r.name])),
            }}
          >
            <SelectItem value={FROM_SCRATCH}>
              {t('roles.fromScratch')}
            </SelectItem>
            {roles.map((r) => (
              <SelectItem key={r.id} value={String(r.id)}>
                {r.name}
              </SelectItem>
            ))}
          </Select>
        </Field>
      </DialogBody>
      <DialogFooter>
        <Button
          variant="secondary"
          onClick={onDone}
          disabled={create.isPending}
        >
          {t('cancel')}
        </Button>
        <Button
          variant="primary"
          loading={create.isPending}
          disabled={!name.trim()}
          onClick={submit}
        >
          {t('roles.create')}
        </Button>
      </DialogFooter>
    </>
  );
}
