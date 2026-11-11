import type { TeamMemberResponse, UserRole } from '@rustrak/client';
import {
  Avatar,
  Button,
  confirm,
  DeleteIcon,
  Menu,
  MoreIcon,
  Tag,
  TeamIcon,
} from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { toast } from 'sonner';
import { useFormatter, useTranslations } from 'use-intl';
import { userMutations } from '@/features/user/api/mutations';
import { Link } from '@/shared/ui/components/link';
import {
  SettingSection,
  SettingsEmpty,
  SettingsListItem,
} from '@/shared/ui/components/setting-row';
import { useToastFailure } from '@/shared/ui/hooks/use-toast-failure';

interface TeamMembersListProps {
  members: TeamMemberResponse[];
  currentUserId: number;
  /** Only an owner makes, unmakes or removes an owner. */
  isOwner: boolean;
  /** `users:delete`. */
  canDelete: boolean;
}

const USER_ROLES: UserRole[] = ['admin', 'member'];

const userRoleLabel = (role: UserRole) =>
  role === 'admin' ? 'roles.admin' : 'roles.member';

/**
 * The instance's people, one row each.
 *
 * "Locked" is the member the reader is, and the primary account. Neither can
 * have their role changed or be removed, so their row has no menu at all
 * rather than a menu of actions that silently fail.
 */
export function TeamMembersList({
  members,
  currentUserId,
  isOwner,
  canDelete,
}: TeamMembersListProps) {
  const t = useTranslations('user');
  const toastFailure = useToastFailure();
  const format = useFormatter();
  const updateRole = useMutation(userMutations.updateUserRole());
  const remove = useMutation(userMutations.removeFromTeam());
  const isPending = updateRole.isPending || remove.isPending;

  const handleRoleChange = (member: TeamMemberResponse, role: UserRole) => {
    updateRole.mutate(
      { userId: member.id, role },
      {
        onSuccess: () =>
          toast.success(t('toast.roleUpdated'), {
            description: t('member.updatedDescription', {
              email: member.email,
              role: t(userRoleLabel(role)),
            }),
          }),
        // `error.message` rather than copy built from `error.fields`: the
        // server does name `role` here, but there is no form on
        // this list to bind that to, and its own sentence ("Cannot demote
        // the last admin") is far more use in a toast than "Role is not
        // valid." would be.
        onError: (error) => toastFailure(t('toast.roleUpdateFailed'), error),
      },
    );
  };

  const handleDelete = async (member: TeamMemberResponse) => {
    const ok = await confirm({
      title: t('removeMember.title', { email: member.email }),
      description: t('removeMember.description'),
      tone: 'danger',
      confirmLabel: t('remove.confirm'),
      cancelLabel: t('cancel'),
    });
    if (!ok) return;
    remove.mutate(member.id, {
      onSuccess: () =>
        toast.success(t('toast.removed'), { description: member.email }),
      onError: (error) => toastFailure(t('toast.removeFailed'), error),
    });
  };

  const actionsFor = (member: TeamMemberResponse) => {
    return [
      ...(isOwner
        ? [
            {
              id: 'role',
              label: t('table.changeRole'),
              disabled: isPending,
              items: USER_ROLES.map((role) => ({
                id: role,
                label: t(userRoleLabel(role)),
                selected: member.role === role,
                onSelect: () => {
                  if (role !== member.role) {
                    handleRoleChange(member, role);
                  }
                },
              })),
            },
          ]
        : []),
      ...(canDelete && (isOwner || member.role !== 'admin')
        ? [
            {
              id: 'remove',
              label: t('remove.confirm'),
              icon: DeleteIcon,
              tone: 'danger' as const,
              separated: true,
              disabled: isPending,
              onSelect: () => handleDelete(member),
            },
          ]
        : []),
    ];
  };

  return (
    <SettingSection
      title={t('membersList.title')}
      description={t('membersList.description')}
    >
      {members.length === 0 ? (
        <SettingsEmpty
          icon={<TeamIcon size="2xl" />}
          title={t('membersList.empty')}
        />
      ) : (
        members.map((member) => {
          const isSelf = member.id === currentUserId;
          const actions = actionsFor(member);
          const locked =
            isSelf || member.is_primary === true || actions.length === 0;
          return (
            <SettingsListItem
              key={member.id}
              leading={<Avatar name={member.email} />}
              title={
                <span className="flex min-w-0 items-center gap-2">
                  <Link
                    href={`/settings/team/${member.id}`}
                    className="truncate hover:underline"
                  >
                    {member.email}
                    {isSelf && (
                      <span className="font-normal text-fg-subtle">
                        {' '}
                        {t('table.you')}
                      </span>
                    )}
                  </Link>
                  <Tag
                    variant="soft"
                    tone={member.role === 'admin' ? 'brand' : 'neutral'}
                  >
                    {t(userRoleLabel(member.role))}
                  </Tag>
                </span>
              }
              description={
                <>
                  {member.is_active ? t('table.active') : t('table.inactive')}
                  {' · '}
                  {member.last_login
                    ? t('table.lastLoginAt', {
                        date: format.dateTime(
                          new Date(member.last_login),
                          'date',
                        ),
                      })
                    : t('table.neverLoggedIn')}
                </>
              }
              actions={
                locked ? null : (
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
                    actions={actions}
                  />
                )
              }
            />
          );
        })
      )}
    </SettingSection>
  );
}
