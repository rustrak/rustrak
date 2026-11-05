import type {
  CatalogResource,
  Permission,
  Role,
  TeamMemberResponse,
  UserAccess,
} from '@rustrak/client';
import {
  Avatar,
  Button,
  DeleteIcon,
  InfoIcon,
  NewIcon,
  SecurityIcon,
  Select,
  SelectItem,
  Tag,
} from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { toast } from 'sonner';
import { useFormatter, useTranslations } from 'use-intl';
import { userMutations } from '@/features/user/api/mutations';
import {
  type AccessRow,
  isUnlisted,
  type RoleChoice,
  type RoleOption,
  reachesProjects,
  roleChoices,
  rowsFrom,
  sameAccess,
  toSetAccess,
} from '@/features/user/model/assignments';
import { grantable } from '@/shared/lib/permission-matrix';
import { PermissionEditor } from '@/shared/ui/components/permission-editor';
import { ProjectsPicker } from '@/shared/ui/components/projects-picker';
import {
  SettingSection,
  SettingsEmpty,
} from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
import { usePermissionsLater } from '@/shared/ui/hooks/use-permissions';

const PERSONAL = 'personal';

type ProjectOption = { id: number; name: string; platform?: string | null };

const missesProjects = (row: AccessRow) =>
  row.role !== null && row.projects?.length === 0;

/**
 * Everything a member can do, on one page: lines of a role and where it
 * applies, and permissions of their own for what no role covers. Edited as a
 * draft and saved at once. Read-only for yourself, since nobody changes their
 * own access, and for a reader without `users:update`.
 */
export function UserAssignments({
  user,
  access,
  roles,
  catalog,
  projects,
  readOnlyReason,
}: {
  user: TeamMemberResponse;
  access: UserAccess;
  roles: Role[];
  catalog: CatalogResource[];
  projects: ProjectOption[];
  /** Why the reader cannot edit, or `null` when they can. */
  readOnlyReason: string | null;
}) {
  const t = useTranslations('access');
  const reader = usePermissionsLater();
  // The callbacks live here, not on `mutate`: a saved access remounts this
  // page, and `mutate`'s own callbacks do not run once it has unmounted.
  const save = useMutation({
    ...userMutations.setAssignments(user.id),
    onSuccess: () => toast.success(t('users.saved')),
    onError: (error) =>
      toast.error(t('users.saveFailed'), {
        description: error.error.message,
      }),
  });
  const [rows, setRows] = useState<AccessRow[]>(() =>
    rowsFrom(access.assignments),
  );
  const [personal, setPersonal] = useState<Permission[]>(
    () => access.personal_permissions,
  );
  const choices = roleChoices(roles, access.assignments, (p) =>
    reader.canAnywhere(p.resource, p.action),
  );
  const isOwner = user.role === 'admin';
  const readOnly = readOnlyReason !== null || isOwner;

  const body = toSetAccess(rows, personal);
  const dirty = !sameAccess(
    body,
    toSetAccess(rowsFrom(access.assignments), access.personal_permissions),
  );
  const hasPersonalRow = rows.some((r) => r.role === 'personal');
  // With no roles to choose from, a line can only be custom permissions.
  const canAddAnother =
    !readOnly && rows.length > 0 && (choices.length > 0 || !hasPersonalRow);

  const permissionsOf = (role: RoleChoice): Permission[] =>
    role === 'personal'
      ? personal
      : (roles.find((r) => r.id === role)?.permissions ?? []);

  const setRow = (index: number, next: Partial<AccessRow>) =>
    setRows((all) =>
      all.map((row, i) => (i === index ? { ...row, ...next } : row)),
    );

  const addRow = () =>
    setRows((all) => [
      ...all,
      { role: choices.length > 0 ? null : 'personal', projects: null },
    ]);

  const chooseRole = (index: number, role: RoleChoice) => {
    // Custom permissions start from the role they replace.
    const draft =
      role === 'personal' && personal.length === 0
        ? permissionsOf(rows[index]?.role ?? null)
        : personal;
    setPersonal(draft);
    const held = role === 'personal' ? draft : permissionsOf(role);
    setRow(
      index,
      reachesProjects(held, catalog) ? { role } : { role, projects: null },
    );
  };

  const customize = (index: number) => {
    setPersonal(permissionsOf(rows[index]?.role ?? null));
    setRow(index, { role: 'personal' });
  };

  return (
    <>
      <MemberHeader
        user={user}
        actions={
          readOnly ? null : (
            <Button
              variant="primary"
              onClick={() => save.mutate(body)}
              loading={save.isPending}
              disabled={!dirty || rows.some(missesProjects)}
            >
              {t('save')}
            </Button>
          )
        }
      />
      {isOwner ? (
        <OwnerAccess />
      ) : (
        <div className="flex flex-col gap-8">
          <ReadOnlyNote reason={readOnlyReason} />
          <SettingSection
            title={t('users.accessTitle')}
            description={t('users.sectionHint')}
            actions={
              canAddAnother ? (
                <Button size="sm" icon={NewIcon} onClick={addRow}>
                  {t('users.addAccess')}
                </Button>
              ) : null
            }
          >
            {rows.length === 0 ? (
              <NoAccess readOnly={readOnly} onGive={addRow} />
            ) : (
              rows.map((row, index) => (
                <AccessLine
                  // A line has no identity of its own until it is saved.
                  // react-doctor-disable-next-line react-doctor/no-array-index-as-key
                  key={index}
                  row={row}
                  roles={choices}
                  projects={projects}
                  // Unknown when the reader cannot list it: where stays shown.
                  reaches={
                    isUnlisted(row.role, roles) ||
                    reachesProjects(permissionsOf(row.role), catalog)
                  }
                  personalTaken={hasPersonalRow && row.role !== 'personal'}
                  unlisted={isUnlisted(row.role, roles)}
                  readOnly={readOnly}
                  onRole={(role) => chooseRole(index, role)}
                  onProjects={(next) => setRow(index, { projects: next })}
                  onCustomize={() => customize(index)}
                  onRemove={() =>
                    setRows((all) => all.filter((_, i) => i !== index))
                  }
                />
              ))
            )}
          </SettingSection>
          {hasPersonalRow ? (
            <CustomPermissions
              catalog={catalog}
              value={personal}
              onChange={setPersonal}
              readOnly={readOnly}
            />
          ) : null}
        </div>
      )}
    </>
  );
}

function ReadOnlyNote({ reason }: { reason: string | null }) {
  return reason ? (
    <p className="flex items-center gap-2 text-fg-subtle text-meta">
      <InfoIcon size="sm" aria-hidden="true" />
      {reason}
    </p>
  ) : null;
}

/** The member's own permissions, limited to what the reader can hand out. */
function CustomPermissions({
  catalog,
  value,
  onChange,
  readOnly,
}: {
  catalog: CatalogResource[];
  value: Permission[];
  onChange: (next: Permission[]) => void;
  readOnly: boolean;
}) {
  const t = useTranslations('access');
  const reader = usePermissionsLater();
  return (
    <section>
      <div className="flex items-center gap-2">
        <h2 className="text-card-title text-fg">{t('users.customTitle')}</h2>
        <Tag variant="soft">
          {t('roles.permissionCount', { count: value.length })}
        </Tag>
      </div>
      <PermissionEditor
        catalog={readOnly ? catalog : grantable(catalog, reader)}
        value={value}
        onChange={onChange}
        readOnly={readOnly}
      />
    </section>
  );
}

function OwnerAccess() {
  const t = useTranslations('access');
  return (
    <SettingSection title={t('users.accessTitle')}>
      <SettingsEmpty
        icon={<SecurityIcon size="2xl" />}
        title={t('users.ownerTitle')}
        description={t('users.ownerNote')}
      />
    </SettingSection>
  );
}

function NoAccess({
  readOnly,
  onGive,
}: {
  readOnly: boolean;
  onGive: () => void;
}) {
  const t = useTranslations('access');
  return (
    <SettingsEmpty
      icon={<SecurityIcon size="2xl" />}
      title={t('users.noAccessTitle')}
      description={t('users.noAccessHint')}
      action={
        readOnly ? null : (
          <Button variant="primary" icon={NewIcon} onClick={onGive}>
            {t('users.giveAccess')}
          </Button>
        )
      }
    />
  );
}

/** Who this page is about, and how they stand. */
function MemberHeader({
  user,
  actions,
}: {
  user: TeamMemberResponse;
  actions: React.ReactNode;
}) {
  const tUser = useTranslations('user');
  const format = useFormatter();
  return (
    <SettingsHeader
      leading={<Avatar name={user.email} size="md" />}
      title={user.email}
      description={
        <span className="flex flex-wrap items-center gap-2">
          <Tag
            variant="soft"
            tone={user.role === 'admin' ? 'brand' : 'neutral'}
          >
            {tUser(`roles.${user.role}`)}
          </Tag>
          <span className="text-meta">
            {user.is_active ? tUser('table.active') : tUser('table.inactive')}
            {' · '}
            {user.last_login
              ? tUser('table.lastLoginAt', {
                  date: format.dateTime(new Date(user.last_login), 'date'),
                })
              : tUser('table.neverLoggedIn')}
          </span>
        </span>
      }
      actions={actions}
    />
  );
}

/** One line of access: what they can do, where, and what that means. */
function AccessLine({
  row,
  roles,
  projects,
  reaches,
  personalTaken,
  unlisted,
  readOnly,
  onRole,
  onProjects,
  onCustomize,
  onRemove,
}: {
  row: AccessRow;
  roles: RoleOption[];
  projects: ProjectOption[];
  /** Whether what this line holds applies to projects at all. */
  reaches: boolean;
  /** Another line already uses the custom permissions. */
  personalTaken: boolean;
  /** Customizing starts from its permissions, which the reader cannot know. */
  unlisted: boolean;
  readOnly: boolean;
  onRole: (role: RoleChoice) => void;
  onProjects: (next: number[] | null) => void;
  onCustomize: () => void;
  onRemove: () => void;
}) {
  const t = useTranslations('access');
  const items = {
    ...Object.fromEntries(roles.map((r) => [String(r.id), r.name])),
    ...(personalTaken ? {} : { [PERSONAL]: t('users.customRole') }),
  };
  const hint = missesProjects(row)
    ? t('users.pickProject')
    : row.role === 'personal'
      ? t('users.customRoleHint')
      : roles.find((r) => r.id === row.role)?.description;

  return (
    <div className="border-border-divider border-b px-5 py-4 last:border-b-0">
      <div className="grid items-start gap-3 md:grid-cols-[minmax(0,20rem)_minmax(0,20rem)_1fr]">
        <div className="min-w-0">
          <p className="mb-1.5 text-fg-secondary text-label">
            {t('users.whatLabel')}
          </p>
          <Select
            value={row.role === null ? null : String(row.role)}
            disabled={readOnly}
            placeholder={t('users.pickRole')}
            aria-label={t('users.whatLabel')}
            items={items}
            onValueChange={(value) =>
              onRole(value === PERSONAL ? 'personal' : Number(value))
            }
          >
            {Object.entries(items).map(([value, label]) => (
              <SelectItem key={value} value={value}>
                {label}
              </SelectItem>
            ))}
          </Select>
        </div>
        <div className="min-w-0">
          <p className="mb-1.5 text-fg-secondary text-label">
            {t('users.whereLabel')}
          </p>
          {reaches ? (
            <ProjectsPicker
              projects={projects}
              value={row.projects}
              onChange={onProjects}
              readOnly={readOnly}
            />
          ) : (
            <p className="flex h-7 items-center text-control text-fg-subtle">
              {row.role === null ? '—' : t('users.everywhere')}
            </p>
          )}
        </div>
        {readOnly ? null : (
          <div className="flex items-center justify-end gap-1 md:mt-6">
            {typeof row.role === 'number' && !personalTaken && !unlisted ? (
              <Button variant="ghost" size="sm" onClick={onCustomize}>
                {t('users.customize')}
              </Button>
            ) : null}
            <Button
              variant="ghost"
              size="sm"
              icon={DeleteIcon}
              aria-label={t('users.removeAccess')}
              onClick={onRemove}
            />
          </div>
        )}
      </div>
      {hint ? (
        <p className="mt-2.5 max-w-3xl text-fg-subtle text-meta">{hint}</p>
      ) : null}
    </div>
  );
}
