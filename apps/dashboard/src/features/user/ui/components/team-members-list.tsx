import type { GlobalRole, TeamMember } from '@rustrak/client';
import { Loader2, Trash2, Users } from 'lucide-react';
import { useState, useTransition } from 'react';
import { toast } from 'sonner';
import { useFormatter, useTranslations } from 'use-intl';
import {
  removeTeamMember,
  updateUserRole,
} from '@/features/user/api/mutations';
import { TeamMembersTable } from '@/features/user/ui/components/team-members-table';
import { invalidate, scope } from '@/shared/api/query-client';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/shared/ui/components/shadcn/alert-dialog';
import { Badge } from '@/shared/ui/components/shadcn/badge';
import { Button } from '@/shared/ui/components/shadcn/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/shared/ui/components/shadcn/card';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/shared/ui/components/shadcn/select';

interface TeamMembersListProps {
  members: TeamMember[];
  currentUserId: number;
}

function RoleBadge({ role }: { role: GlobalRole }) {
  const t = useTranslations('user');
  return (
    <Badge variant={role === 'admin' ? 'default' : 'secondary'}>
      {role === 'admin' ? t('roles.admin') : t('roles.member')}
    </Badge>
  );
}

function RoleSelect({
  member,
  disabled,
  onChange,
}: {
  member: TeamMember;
  disabled: boolean;
  onChange: (role: GlobalRole) => void;
}) {
  const t = useTranslations('user');
  return (
    <Select
      value={member.role}
      onValueChange={(value) => {
        if (value && value !== member.role) {
          onChange(value as GlobalRole);
        }
      }}
      disabled={disabled}
    >
      <SelectTrigger
        size="sm"
        className="w-32"
        aria-label={t('table.changeRoleAria', { email: member.email })}
      >
        <SelectValue>
          {(value) => t(value === 'admin' ? 'roles.admin' : 'roles.member')}
        </SelectValue>
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="admin">{t('roles.admin')}</SelectItem>
        <SelectItem value="member">{t('roles.member')}</SelectItem>
      </SelectContent>
    </Select>
  );
}

export function TeamMembersList({
  members,
  currentUserId,
}: TeamMembersListProps) {
  const t = useTranslations('user');
  const format = useFormatter();
  const [isPending, startTransition] = useTransition();
  const [memberToDelete, setMemberToDelete] = useState<TeamMember | null>(null);

  const handleRoleChange = (member: TeamMember, role: GlobalRole) => {
    startTransition(async () => {
      const result = await updateUserRole(member.id, role);
      if (result.success) {
        toast.success(t('toast.roleUpdated'), {
          description: t('member.updatedDescription', {
            email: member.email,
            role: t(role === 'admin' ? 'roles.admin' : 'roles.member'),
          }),
        });
        void invalidate(scope.team);
      } else {
        // `error.message` rather than copy built from `error.fields`: the
        // server does name `role` here, but there is no react-hook-form on
        // this table to bind that to, and its own sentence ("Cannot demote
        // the last admin") is far more use in a toast than "Role is not
        // valid." would be.
        toast.error(t('toast.roleUpdateFailed'), {
          description: result.error.message,
        });
      }
    });
  };

  const handleConfirmDelete = () => {
    if (!memberToDelete) return;
    const member = memberToDelete;
    startTransition(async () => {
      const result = await removeTeamMember(member.id);
      if (result.success) {
        toast.success(t('toast.removed'), { description: member.email });
        setMemberToDelete(null);
        void invalidate(scope.team);
      } else {
        toast.error(t('toast.removeFailed'), {
          description: result.error.message,
        });
      }
    });
  };

  if (members.length === 0) {
    return (
      <Card className="border-dashed">
        <CardContent className="flex flex-col items-center justify-center py-12">
          <Users className="size-12 text-muted-foreground/50 mb-4" />
          <p className="text-muted-foreground">{t('membersList.empty')}</p>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('membersList.title')}</CardTitle>
        <CardDescription>{t('membersList.description')}</CardDescription>
      </CardHeader>
      <CardContent>
        {/* Mobile: card list */}
        <div className="md:hidden space-y-3">
          {members.map((member) => {
            const isSelf = member.id === currentUserId;
            const isPrimary = member.is_primary === true;
            const locked = isSelf || isPrimary;
            return (
              <div key={member.id} className="rounded-lg border p-3 space-y-3">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0 space-y-1">
                    <p className="text-sm font-medium truncate">
                      {member.email}
                      {isSelf && (
                        <span className="text-muted-foreground">
                          {' '}
                          {t('table.you')}
                        </span>
                      )}
                    </p>
                    <p className="text-xs text-muted-foreground">
                      {member.is_active
                        ? t('table.active')
                        : t('table.inactive')}
                      {' · '}
                      {member.last_login
                        ? t('table.lastLoginAt', {
                            date: format.dateTime(
                              new Date(member.last_login),
                              'date',
                            ),
                          })
                        : t('table.neverLoggedIn')}
                    </p>
                  </div>
                  <RoleBadge role={member.role} />
                </div>
                <div className="flex items-center gap-2">
                  <RoleSelect
                    member={member}
                    disabled={isPending || locked}
                    onChange={(role) => handleRoleChange(member, role)}
                  />
                  {!locked && (
                    <Button
                      variant="destructive"
                      size="icon"
                      onClick={() => setMemberToDelete(member)}
                      disabled={isPending}
                      aria-label={t('table.removeAria', {
                        email: member.email,
                      })}
                    >
                      <Trash2 className="size-4" />
                    </Button>
                  )}
                </div>
              </div>
            );
          })}
        </div>

        {/* Desktop: table */}
        <TeamMembersTable
          members={members}
          currentUserId={currentUserId}
          disabled={isPending}
          renderRoleBadge={(role) => <RoleBadge role={role} />}
          renderRoleSelect={(member, locked) => (
            <RoleSelect
              member={member}
              disabled={isPending || locked}
              onChange={(role) => handleRoleChange(member, role)}
            />
          )}
          onRemove={setMemberToDelete}
        />
      </CardContent>

      <AlertDialog
        open={memberToDelete !== null}
        onOpenChange={(open) => {
          if (!open) setMemberToDelete(null);
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t('removeMember.title', { email: memberToDelete?.email ?? '' })}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t('removeMember.description')}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={isPending}>
              {t('cancel')}
            </AlertDialogCancel>
            <AlertDialogAction
              onClick={handleConfirmDelete}
              disabled={isPending}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              {isPending ? (
                <>
                  <Loader2 className="mr-2 size-4 animate-spin" />
                  {t('remove.removing')}
                </>
              ) : (
                t('remove.confirm')
              )}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Card>
  );
}
