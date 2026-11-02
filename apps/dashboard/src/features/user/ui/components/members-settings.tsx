import type {
  ProjectMemberResponse,
  ProjectRole,
  TeamMemberResponse,
} from '@rustrak/client';
import {
  Avatar,
  Button,
  confirm,
  DeleteIcon,
  Dialog,
  DialogBody,
  DialogFooter,
  DialogHeader,
  Menu,
  MoreIcon,
  NewIcon,
  Select,
  SelectItem,
  Tag,
  TeamIcon,
} from '@rustrak/ui';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { userMutations } from '@/features/user/api/mutations';
import { userQueries } from '@/features/user/api/queries';
import { PROJECT_ROLES, roleLabel } from '@/features/user/model/roles';
import {
  SettingSection,
  SettingsEmpty,
  SettingsListItem,
} from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';

interface MembersSettingsProps {
  projectId: number;
  members: ProjectMemberResponse[];
  currentUserId?: number;
  canManage: boolean;
}

export function MembersSettings({
  projectId,
  members,
  currentUserId,
  canManage,
}: MembersSettingsProps) {
  const t = useTranslations('user');
  const upsert = useMutation(userMutations.upsertMember(projectId));
  const remove = useMutation(userMutations.removeMember(projectId));
  const isPending = upsert.isPending || remove.isPending;
  // Only a manager sees the add-member dropdown it populates.
  const team = useQuery({ ...userQueries.team(), enabled: canManage });
  const [showAddDialog, setShowAddDialog] = useState(false);

  const memberIds = new Set(members.map((m) => m.user_id));
  // Exclude users who are already members, and global admins — admins (including
  // the primary user) already have implicit access to every project, so adding
  // them as a project member is meaningless.
  const availableUsers = (team.data ?? []).filter(
    (user) => !memberIds.has(user.id) && user.role !== 'admin',
  );

  const handleRoleChange = (
    member: ProjectMemberResponse,
    role: ProjectRole,
  ) => {
    if (role === member.role) return;
    upsert.mutate(
      { user_id: member.user_id, role },
      {
        onSuccess: () =>
          toast.success(t('toast.updated'), {
            description: t('member.updatedDescription', {
              email: member.email,
              role: t(roleLabel(role)),
            }),
          }),
        // `error.message` rather than copy built from `error.fields`: the
        // server names `role`, but these are row menu actions, not a
        // react-hook-form, and the server's own sentence ("Cannot downgrade
        // the last project admin") says far more than the generic field copy.
        onError: (error) =>
          toast.error(t('toast.updateFailed'), {
            description: error.error.message,
          }),
      },
    );
  };

  const handleRemove = async (member: ProjectMemberResponse) => {
    const ok = await confirm({
      title: t('remove.title'),
      description: t('remove.description', { email: member.email }),
      tone: 'danger',
      confirmLabel: t('remove.confirm'),
      cancelLabel: t('cancel'),
    });
    if (!ok) return;
    remove.mutate(member.user_id, {
      onSuccess: () => toast.success(t('toast.removed')),
      onError: (error) =>
        toast.error(t('toast.removeFailed'), {
          description: error.error.message,
        }),
    });
  };

  return (
    <>
      <SettingsHeader
        title={t('members.title')}
        description={
          canManage
            ? t('members.manageDescription')
            : t('members.readOnlyDescription')
        }
        actions={
          canManage ? (
            <Button
              variant="primary"
              icon={NewIcon}
              onClick={() => setShowAddDialog(true)}
            >
              {t('members.addMember')}
            </Button>
          ) : null
        }
      />

      <SettingSection title={t('membersList.title')}>
        {members.length === 0 ? (
          <SettingsEmpty
            icon={<TeamIcon size="2xl" />}
            title={t('members.empty')}
          />
        ) : (
          // The ⋯ menu exists only when the reader can manage members: a
          // menu that is absent rather than one full of disabled actions.
          members.map((member) => (
            <SettingsListItem
              key={member.user_id}
              leading={<Avatar name={member.email} />}
              // The role sits beside the name rather than in `meta`, which
              // hides below `sm`: on a phone it is the row's one fact.
              title={
                <span className="flex min-w-0 items-center gap-2">
                  <span className="truncate">
                    {member.email}
                    {member.user_id === currentUserId && (
                      <span className="font-normal text-fg-subtle">
                        {' '}
                        {t('table.you')}
                      </span>
                    )}
                  </span>
                  <Tag
                    variant="soft"
                    tone={member.role === 'admin' ? 'brand' : 'neutral'}
                  >
                    {t(roleLabel(member.role))}
                  </Tag>
                </span>
              }
              actions={
                canManage ? (
                  <Menu
                    align="end"
                    trigger={
                      <Button
                        variant="ghost"
                        size="sm"
                        icon={MoreIcon}
                        aria-label={t('table.actionsAria')}
                      />
                    }
                    actions={[
                      {
                        id: 'role',
                        label: t('table.changeRole'),
                        disabled: isPending,
                        items: PROJECT_ROLES.map((entry) => ({
                          id: entry.value,
                          label: t(entry.labelKey),
                          selected: member.role === entry.value,
                          onSelect: () => handleRoleChange(member, entry.value),
                        })),
                      },
                      {
                        id: 'remove',
                        label: t('remove.confirm'),
                        icon: DeleteIcon,
                        tone: 'danger',
                        separated: true,
                        disabled: isPending,
                        onSelect: () => handleRemove(member),
                      },
                    ]}
                  />
                ) : null
              }
            />
          ))
        )}
      </SettingSection>

      <AddMemberDialog
        open={showAddDialog}
        onOpenChange={setShowAddDialog}
        projectId={projectId}
        availableUsers={availableUsers}
        // An empty dropdown reading "No users available" is a claim about the
        // team; a failed read gives no basis to make it.
        teamFailed={team.isError}
        onSuccess={() => setShowAddDialog(false)}
      />
    </>
  );
}

// ============================================================================
// Add Member Dialog
// ============================================================================

interface AddMemberDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  projectId: number;
  availableUsers: TeamMemberResponse[];
  /** The team list could not be read, so the dropdown is empty for a reason. */
  teamFailed: boolean;
  onSuccess: () => void;
}

function AddMemberDialog({
  open,
  onOpenChange,
  ...formProps
}: AddMemberDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange} size="md">
      <AddMemberForm onOpenChange={onOpenChange} {...formProps} />
    </Dialog>
  );
}

/**
 * The dialog body, and the only owner of the draft member.
 *
 * It lives inside `Dialog`'s popup rather than beside it because Base UI
 * unmounts the portal once the close animation finishes. Mounting is
 * therefore the reset: every open starts from a fresh `userId` and `role`
 * with no effect clearing them, and the previous draft stays on screen
 * while the dialog animates away instead of blanking mid-flight.
 */
function AddMemberForm({
  onOpenChange,
  projectId,
  availableUsers,
  teamFailed,
  onSuccess,
}: Omit<AddMemberDialogProps, 'open'>) {
  const t = useTranslations('user');
  const add = useMutation(userMutations.upsertMember(projectId));
  const isPending = add.isPending;
  const [userId, setUserId] = useState('');
  const [role, setRole] = useState<ProjectRole>('viewer');

  const handleAdd = () => {
    const parsedId = Number.parseInt(userId, 10);
    if (!Number.isFinite(parsedId)) return;
    add.mutate(
      { user_id: parsedId, role },
      {
        onSuccess: () => {
          toast.success(t('toast.added'));
          onSuccess();
        },
        onError: (error) =>
          toast.error(t('toast.addFailed'), {
            description: error.error.message,
          }),
      },
    );
  };

  return (
    <>
      <DialogHeader
        title={t('addMember.title')}
        description={t('addMember.description')}
      />

      <DialogBody className="flex flex-col gap-4">
        <div className="flex flex-col gap-1.5">
          <label
            htmlFor="add-member-user"
            className="text-label text-fg-secondary"
          >
            {t('addMember.userLabel')}
          </label>
          <Select
            id="add-member-user"
            aria-label={t('addMember.userAria')}
            value={userId || null}
            onValueChange={setUserId}
            disabled={isPending || availableUsers.length === 0}
            items={Object.fromEntries(
              availableUsers.map((u) => [String(u.id), u.email]),
            )}
            placeholder={
              teamFailed
                ? t('addMember.teamFailed')
                : availableUsers.length === 0
                  ? t('addMember.noUsers')
                  : t('addMember.selectUser')
            }
          >
            {availableUsers.map((user) => (
              <SelectItem key={user.id} value={String(user.id)}>
                {user.email}
              </SelectItem>
            ))}
          </Select>
        </div>

        <div className="flex flex-col gap-1.5">
          <label
            htmlFor="add-member-role"
            className="text-label text-fg-secondary"
          >
            {t('addMember.roleLabel')}
          </label>
          <Select<ProjectRole>
            id="add-member-role"
            aria-label={t('addMember.roleAria')}
            value={role}
            onValueChange={setRole}
            disabled={isPending}
            items={Object.fromEntries(
              PROJECT_ROLES.map((r) => [r.value, t(r.labelKey)]),
            )}
          >
            {PROJECT_ROLES.map((r) => (
              <SelectItem key={r.value} value={r.value}>
                {t(r.labelKey)}
              </SelectItem>
            ))}
          </Select>
        </div>
      </DialogBody>

      <DialogFooter>
        <Button
          variant="secondary"
          onClick={() => onOpenChange(false)}
          disabled={isPending}
        >
          {t('cancel')}
        </Button>
        <Button
          variant="primary"
          icon={NewIcon}
          loading={isPending}
          onClick={handleAdd}
          disabled={!userId}
        >
          {t('addMember.submit')}
        </Button>
      </DialogFooter>
    </>
  );
}
