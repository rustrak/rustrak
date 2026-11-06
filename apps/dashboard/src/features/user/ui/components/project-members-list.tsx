import type { MemberCandidates, ProjectMember } from '@rustrak/client';
import {
  Avatar,
  Button,
  confirm,
  DeleteIcon,
  Dialog,
  DialogBody,
  DialogClose,
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
import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { userMutations } from '@/features/user/api/mutations';
import {
  SettingSection,
  SettingsEmpty,
  SettingsListItem,
} from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';

interface ProjectMembersListProps {
  projectId: number;
  members: ProjectMember[];
  /** Who and what can be given; `null` until known, or without the permission. */
  candidates: MemberCandidates | null;
  canUpdate: boolean;
  currentUserId: number;
}

/** A project's members and their roles, with who may change them. */
export function ProjectMembersList({
  projectId,
  members,
  candidates,
  canUpdate,
  currentUserId,
}: ProjectMembersListProps) {
  const t = useTranslations('user');
  const remove = useMutation(userMutations.removeProjectMember(projectId));
  // `undefined`: closed. `null`: adding someone new.
  const [editing, setEditing] = useState<ProjectMember | null | undefined>();

  const handleRemove = async (member: ProjectMember) => {
    const ok = await confirm({
      title: t('remove.title'),
      description: t('remove.description', { email: member.email }),
      tone: 'danger',
      confirmLabel: t('remove.confirm'),
      cancelLabel: t('cancel'),
    });
    if (!ok) return;
    remove.mutate(member.user_id, {
      onSuccess: () =>
        toast.success(t('toast.removed'), { description: member.email }),
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
          canUpdate
            ? t('members.manageDescription')
            : t('members.readOnlyDescription')
        }
        actions={
          canUpdate && candidates ? (
            <Button
              variant="primary"
              icon={NewIcon}
              onClick={() => setEditing(null)}
            >
              {t('members.addMember')}
            </Button>
          ) : null
        }
      />
      <SettingSection title={t('members.title')}>
        {members.length === 0 ? (
          <SettingsEmpty
            icon={<TeamIcon size="2xl" />}
            title={t('members.empty')}
          />
        ) : (
          members.map((member) => (
            <SettingsListItem
              key={member.user_id}
              leading={<Avatar name={member.email} />}
              title={
                <span className="flex min-w-0 flex-wrap items-center gap-2">
                  <span className="truncate">
                    {member.email}
                    {member.user_id === currentUserId ? (
                      <span className="font-normal text-fg-subtle">
                        {' '}
                        {t('table.you')}
                      </span>
                    ) : null}
                  </span>
                  {member.roles.map((role) => (
                    <Tag key={role.id} variant="soft" tone="neutral">
                      {role.name}
                    </Tag>
                  ))}
                </span>
              }
              actions={
                canUpdate && member.user_id !== currentUserId ? (
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
                      ...(candidates
                        ? [
                            {
                              id: 'role',
                              label: t('table.changeRole'),
                              onSelect: () => setEditing(member),
                            },
                          ]
                        : []),
                      {
                        id: 'remove',
                        label: t('remove.confirm'),
                        icon: DeleteIcon,
                        tone: 'danger' as const,
                        separated: true,
                        disabled: remove.isPending,
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
      <Dialog
        open={editing !== undefined}
        onOpenChange={(open) => {
          if (!open) setEditing(undefined);
        }}
        size="sm"
      >
        {candidates && editing !== undefined ? (
          <MemberForm
            projectId={projectId}
            member={editing}
            candidates={candidates}
            onDone={() => setEditing(undefined)}
          />
        ) : null}
      </Dialog>
    </>
  );
}

/**
 * Who, and with which role.
 *
 * ponytail: one role at a time, replacing what they held on this project; the
 * team page assigns several. Pick several here if project admins ask for it.
 */
function MemberForm({
  projectId,
  member,
  candidates,
  onDone,
}: {
  projectId: number;
  /** `null`: someone new, chosen here. */
  member: ProjectMember | null;
  candidates: MemberCandidates;
  onDone: () => void;
}) {
  const t = useTranslations('user');
  const set = useMutation(userMutations.setProjectMember(projectId));
  const [userId, setUserId] = useState(member ? String(member.user_id) : '');
  const [roleId, setRoleId] = useState(
    member?.roles[0] ? String(member.roles[0].id) : '',
  );
  const userItems = Object.fromEntries(
    candidates.users.map((u) => [String(u.id), u.email]),
  );
  const roleItems = Object.fromEntries(
    candidates.roles.map((r) => [String(r.id), r.name]),
  );

  const submit = (event: React.FormEvent) => {
    event.preventDefault();
    set.mutate(
      { user_id: Number(userId), role_ids: [Number(roleId)] },
      {
        onSuccess: () => {
          toast.success(member ? t('toast.updated') : t('toast.added'), {
            description: userItems[userId] ?? member?.email,
          });
          onDone();
        },
        onError: (error) =>
          toast.error(member ? t('toast.updateFailed') : t('toast.addFailed'), {
            description: error.error.message,
          }),
      },
    );
  };

  return (
    <form onSubmit={submit}>
      <DialogHeader
        title={member ? t('table.changeRole') : t('addMember.title')}
        description={member ? member.email : t('addMember.description')}
      />
      <DialogBody className="flex flex-col gap-4">
        {member ? null : (
          <label className="flex flex-col gap-2 text-meta text-fg-secondary">
            {t('addMember.userLabel')}
            <Select
              value={userId}
              onValueChange={(value) => setUserId(String(value))}
              aria-label={t('addMember.userAria')}
              placeholder={
                candidates.users.length > 0
                  ? t('addMember.selectUser')
                  : t('addMember.noUsers')
              }
              items={userItems}
            >
              {Object.entries(userItems).map(([value, label]) => (
                <SelectItem key={value} value={value}>
                  {label}
                </SelectItem>
              ))}
            </Select>
          </label>
        )}
        <label className="flex flex-col gap-2 text-meta text-fg-secondary">
          {t('addMember.roleLabel')}
          <Select
            value={roleId}
            onValueChange={(value) => setRoleId(String(value))}
            aria-label={t('addMember.roleAria')}
            items={roleItems}
          >
            {Object.entries(roleItems).map(([value, label]) => (
              <SelectItem key={value} value={value}>
                {label}
              </SelectItem>
            ))}
          </Select>
        </label>
      </DialogBody>
      <DialogFooter>
        <DialogClose
          render={<Button variant="secondary">{t('cancel')}</Button>}
        />
        <Button
          type="submit"
          variant="primary"
          disabled={!userId || !roleId || set.isPending}
        >
          {member ? t('members.save') : t('addMember.submit')}
        </Button>
      </DialogFooter>
    </form>
  );
}
