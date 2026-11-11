import type {
  CatalogResource,
  CreateAuthToken,
  Permission,
} from '@rustrak/client';
import {
  Button,
  CopyIcon,
  Dialog,
  DialogBody,
  DialogFooter,
  DialogHeader,
  Field,
  FieldHint,
  FieldLabel,
  Input,
  NewIcon,
  Select,
  SelectedIcon,
  SelectItem,
} from '@rustrak/ui';
import { useState } from 'react';
import { useTranslations } from 'use-intl';
import { tokenExpiry } from '@/features/token/model/expiry';
import { grantable } from '@/shared/lib/permission-matrix';
import { PermissionEditor } from '@/shared/ui/components/permission-editor';
import {
  type ProjectOption,
  ProjectsPicker,
} from '@/shared/ui/components/projects-picker';
import { useCopy } from '@/shared/ui/hooks/use-copy';
import { usePermissionsLater } from '@/shared/ui/hooks/use-permissions';

const TOKEN_DESCRIPTION_MAX_LENGTH = 200;

const ALL = 'all';
const CUSTOM = 'custom';

/**
 * Creating a token, and then showing the one thing the server will not repeat
 * on demand in this shape: the full value, once.
 *
 * Two states in one dialog because they are two steps of one act. Closing
 * after the value is shown is what forgets it, which is why the caller gets
 * `onDone` rather than a plain `onOpenChange`.
 */
export function CreateTokenDialog({
  open,
  onOpenChange,
  newToken,
  isPending,
  onCreate,
  onDone,
  catalog,
  projects,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The full value, present only between creating it and closing the dialog. */
  newToken: string | null;
  isPending: boolean;
  onCreate: (input: CreateAuthToken) => void;
  onDone: () => void;
  catalog: CatalogResource[];
  projects: ProjectOption[];
}) {
  const t = useTranslations('tokens');
  const reader = usePermissionsLater();
  const [description, setDescription] = useState('');
  const { copied, copy } = useCopy();
  // `null` is "everything its owner may do" / "every project": the default,
  // and what the server stores when the field is left out.
  const [permissions, setPermissions] = useState<Permission[] | null>(null);
  const [projectIds, setProjectIds] = useState<number[] | null>(null);
  const [expiresOn, setExpiresOn] = useState('');

  const trimmed = description.trim();
  const isValid = trimmed.length <= TOKEN_DESCRIPTION_MAX_LENGTH;
  const expiry = tokenExpiry(expiresOn);
  // Every way to submit goes through this: the button, and Enter.
  const canSubmit =
    !isPending &&
    isValid &&
    expiry.ok &&
    permissions?.length !== 0 &&
    projectIds?.length !== 0;

  const create = () => {
    if (!canSubmit) return;
    onCreate({
      description: trimmed || undefined,
      permissions,
      project_ids: projectIds,
      expires_at: expiry.ok ? expiry.value : null,
    });
  };

  // The draft goes only when the dialog does: a failed creation stays open to
  // retry, and clearing it there would retry with no limits at all.
  const forgetDraft = () => {
    setDescription('');
    setPermissions(null);
    setProjectIds(null);
    setExpiresOn('');
  };

  const close = () => {
    // Dropping a creation in flight would lose the only copy of its token.
    if (isPending) return;
    forgetDraft();
    onOpenChange(false);
  };

  const permissionChoices = {
    [ALL]: t('create.allPermissions'),
    [CUSTOM]: t('create.customPermissions'),
  };

  return (
    <>
      <Button
        variant="primary"
        icon={NewIcon}
        onClick={() => onOpenChange(true)}
      >
        {t('create.trigger')}
      </Button>
      <Dialog
        open={open}
        onOpenChange={(next) => (next ? onOpenChange(true) : close())}
        size={newToken ? 'sm' : 'xl'}
      >
        {newToken ? (
          <>
            <DialogHeader
              title={t('create.createdTitle')}
              description={t('create.createdDescription')}
            />
            <DialogBody>
              <div className="flex items-center gap-2 rounded-lg border border-border-subtle bg-surface p-3">
                <code className="flex-1 break-all font-mono text-mono text-fg">
                  {newToken}
                </code>
                <Button
                  variant="ghost"
                  size="sm"
                  icon={copied ? SelectedIcon : CopyIcon}
                  onClick={() => newToken && copy(newToken)}
                  className="shrink-0"
                  aria-label={t('create.copyTokenLabel')}
                />
              </div>
              <p className="mt-2 text-hint text-fg-subtle">
                {t('create.copyAgainHint')}
              </p>
            </DialogBody>
            <DialogFooter>
              <Button
                variant="primary"
                onClick={() => {
                  forgetDraft();
                  onDone();
                }}
              >
                {t('create.done')}
              </Button>
            </DialogFooter>
          </>
        ) : (
          <>
            <DialogHeader
              title={t('create.title')}
              description={t('create.description')}
            />
            <DialogBody>
              <div className="grid gap-x-4 gap-y-4 sm:grid-cols-2">
                <Field>
                  <FieldLabel>{t('create.descriptionLabel')}</FieldLabel>
                  <Input
                    placeholder={t('create.descriptionPlaceholder')}
                    value={description}
                    onChange={(e) => setDescription(e.target.value)}
                    onKeyDown={(e) => e.key === 'Enter' && create()}
                    // Deliberately above the limit: the field accepts a little
                    // overshoot so the message can say what is wrong, rather
                    // than silently swallowing keystrokes at the boundary.
                    maxLength={TOKEN_DESCRIPTION_MAX_LENGTH + 10}
                    invalid={!isValid}
                  />
                  {isValid ? null : (
                    <p className="text-danger-fg text-meta">
                      {t('create.descriptionTooLong', {
                        max: TOKEN_DESCRIPTION_MAX_LENGTH,
                      })}
                    </p>
                  )}
                </Field>
                <Field>
                  <FieldLabel>{t('create.expiresLabel')}</FieldLabel>
                  <Input
                    type="date"
                    value={expiresOn}
                    onChange={(e) => setExpiresOn(e.target.value)}
                    invalid={!expiry.ok}
                  />
                  {expiry.ok ? (
                    <FieldHint>{t('create.expiresHint')}</FieldHint>
                  ) : (
                    <p className="text-danger-fg text-meta">
                      {t('create.expiresInPast')}
                    </p>
                  )}
                </Field>
                <Field>
                  <FieldLabel>{t('create.permissionsLabel')}</FieldLabel>
                  <Select
                    value={permissions === null ? ALL : CUSTOM}
                    aria-label={t('create.permissionsLabel')}
                    items={permissionChoices}
                    onValueChange={(value) =>
                      setPermissions(value === ALL ? null : [])
                    }
                  >
                    {Object.entries(permissionChoices).map(([value, label]) => (
                      <SelectItem key={value} value={value}>
                        {label}
                      </SelectItem>
                    ))}
                  </Select>
                  <FieldHint>{t('create.permissionsHint')}</FieldHint>
                </Field>
                <Field>
                  <FieldLabel>{t('create.projectsLabel')}</FieldLabel>
                  <ProjectsPicker
                    projects={projects}
                    value={projectIds}
                    onChange={setProjectIds}
                    readOnly={false}
                    allLabel={t('create.allProjects')}
                    allHint={t('create.allProjectsHint')}
                  />
                  <FieldHint>{t('create.projectsHint')}</FieldHint>
                </Field>
              </div>
              {permissions !== null ? (
                <div className="mt-4">
                  <PermissionEditor
                    catalog={grantable(catalog, reader)}
                    value={permissions}
                    onChange={setPermissions}
                    // The same height on every tab, and the dialog stops growing.
                    panelClassName="h-64 overflow-y-auto"
                  />
                </div>
              ) : null}
            </DialogBody>
            <DialogFooter>
              <Button variant="ghost" onClick={close} disabled={isPending}>
                {t('create.cancel')}
              </Button>
              <Button variant="primary" onClick={create} disabled={!canSubmit}>
                {isPending ? t('create.creating') : t('create.submit')}
              </Button>
            </DialogFooter>
          </>
        )}
      </Dialog>
    </>
  );
}
