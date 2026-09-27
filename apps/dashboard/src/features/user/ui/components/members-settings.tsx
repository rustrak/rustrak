import type { ProjectMember, ProjectRole, TeamMember } from '@rustrak/client';
import { useQuery } from '@tanstack/react-query';
import { Loader2, UserPlus, Users } from 'lucide-react';
import { useState, useTransition } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import {
  removeProjectMember,
  upsertProjectMember,
} from '@/features/user/api/mutations';
import { userQueries } from '@/features/user/api/queries';
import { PROJECT_ROLES, roleLabel } from '@/features/user/model/roles';
import { ProjectMembersTable } from '@/features/user/ui/components/project-members-table';
import { invalidateProject } from '@/shared/api/query-client';
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
import { Button } from '@/shared/ui/components/shadcn/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/shared/ui/components/shadcn/dialog';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/shared/ui/components/shadcn/select';

interface MembersSettingsProps {
  projectId: number;
  members: ProjectMember[];
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
  const [isPending, startTransition] = useTransition();
  // The roster fills the add-member dropdown, and only a manager sees it.
  const { data: teamResult } = useQuery({
    ...userQueries.team(),
    enabled: canManage,
  });
  const team: TeamMember[] | null = teamResult?.success
    ? teamResult.data
    : null;
  // Distinct from `team === null`, which only means "not loaded yet". An empty
  // dropdown reading "No users available" is a claim about the team; if the
  // fetch failed we have no basis to make it.
  const teamFailed = teamResult?.success === false;
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [removingMember, setRemovingMember] = useState<ProjectMember | null>(
    null,
  );

  const memberIds = new Set(members.map((m) => m.user_id));
  // Exclude users who are already members, and global admins — admins (including
  // the primary user) already have implicit access to every project, so adding
  // them as a project member is meaningless.
  const availableUsers = (team ?? []).filter(
    (user) => !memberIds.has(user.id) && user.role !== 'admin',
  );

  const handleRoleChange = (member: ProjectMember, role: ProjectRole) => {
    if (role === member.role) return;
    startTransition(async () => {
      const result = await upsertProjectMember(projectId, {
        user_id: member.user_id,
        role,
      });
      if (result.success) {
        toast.success(t('toast.updated'), {
          description: t('member.updatedDescription', {
            email: member.email,
            role: t(roleLabel(role)),
          }),
        });
        await invalidateProject(projectId);
      } else {
        // `error.message` rather than copy built from `error.fields`: the
        // server names `role`, but these are table row selects, not a
        // react-hook-form, and the server's own sentence ("Cannot downgrade
        // the last project admin") says far more than the generic field copy.
        toast.error(t('toast.updateFailed'), {
          description: result.error.message,
        });
      }
    });
  };

  const handleRemove = () => {
    if (!removingMember) return;
    startTransition(async () => {
      const result = await removeProjectMember(
        projectId,
        removingMember.user_id,
      );
      if (result.success) {
        toast.success(t('toast.removed'));
        setRemovingMember(null);
        await invalidateProject(projectId);
      } else {
        toast.error(t('toast.removeFailed'), {
          description: result.error.message,
        });
      }
    });
  };

  return (
    <>
      <div className="mb-6 flex items-start justify-between gap-4 md:mb-8">
        <div>
          <h1 className="text-xl font-extrabold tracking-tight md:text-2xl">
            {t('members.title')}
          </h1>
          <p className="mt-1 text-muted-foreground">
            {canManage
              ? t('members.manageDescription')
              : t('members.readOnlyDescription')}
          </p>
        </div>
        {canManage && (
          <Button onClick={() => setShowAddDialog(true)} className="shrink-0">
            <UserPlus className="mr-2 size-4" />
            {t('members.addMember')}
          </Button>
        )}
      </div>

      {members.length === 0 ? (
        <div className="rounded-lg border border-dashed py-16 text-center">
          <Users className="mx-auto mb-3 size-10 text-muted-foreground/50" />
          <p className="text-sm text-muted-foreground">{t('members.empty')}</p>
        </div>
      ) : (
        <ProjectMembersTable
          members={members}
          currentUserId={currentUserId}
          canManage={canManage}
          disabled={isPending}
          onRoleChange={handleRoleChange}
          onRemove={setRemovingMember}
        />
      )}

      <AddMemberDialog
        open={showAddDialog}
        onOpenChange={setShowAddDialog}
        projectId={projectId}
        availableUsers={availableUsers}
        teamFailed={teamFailed}
        onSuccess={() => {
          setShowAddDialog(false);
          void invalidateProject(projectId);
        }}
      />

      {/* Remove confirmation */}
      <AlertDialog
        open={!!removingMember}
        onOpenChange={(open) => !open && setRemovingMember(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t('remove.title')}</AlertDialogTitle>
            <AlertDialogDescription>
              {t('remove.description', {
                email: removingMember?.email ?? '',
              })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={isPending}>
              {t('cancel')}
            </AlertDialogCancel>
            <AlertDialogAction
              onClick={handleRemove}
              disabled={isPending}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              {isPending ? t('remove.removing') : t('remove.confirm')}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
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
  availableUsers: TeamMember[];
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
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <AddMemberForm onOpenChange={onOpenChange} {...formProps} />
      </DialogContent>
    </Dialog>
  );
}

/**
 * The dialog body, and the only owner of the draft member.
 *
 * It lives inside `DialogContent` rather than beside it because Base UI
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
  const [isPending, startTransition] = useTransition();
  const [userId, setUserId] = useState('');
  const [role, setRole] = useState<ProjectRole>('viewer');

  const handleAdd = () => {
    const parsedId = Number.parseInt(userId, 10);
    if (!Number.isFinite(parsedId)) return;
    startTransition(async () => {
      const result = await upsertProjectMember(projectId, {
        user_id: parsedId,
        role,
      });
      if (result.success) {
        toast.success(t('toast.added'));
        onSuccess();
      } else {
        toast.error(t('toast.addFailed'), {
          description: result.error.message,
        });
      }
    });
  };

  return (
    <>
      <DialogHeader>
        <DialogTitle>{t('addMember.title')}</DialogTitle>
        <DialogDescription>{t('addMember.description')}</DialogDescription>
      </DialogHeader>

      <div className="space-y-4 py-1">
        <div className="space-y-1.5">
          <label
            htmlFor="add-member-user"
            className="text-xs font-bold uppercase tracking-widest text-muted-foreground"
          >
            {t('addMember.userLabel')}
          </label>
          <Select
            value={userId}
            onValueChange={(value) => setUserId(value ?? '')}
            disabled={isPending || availableUsers.length === 0}
          >
            <SelectTrigger
              id="add-member-user"
              className="w-full"
              aria-label={t('addMember.userAria')}
            >
              <SelectValue
                placeholder={
                  teamFailed
                    ? t('addMember.teamFailed')
                    : availableUsers.length === 0
                      ? t('addMember.noUsers')
                      : t('addMember.selectUser')
                }
              >
                {(value) =>
                  availableUsers.find((u) => String(u.id) === value)?.email ??
                  ''
                }
              </SelectValue>
            </SelectTrigger>
            <SelectContent>
              {availableUsers.map((user) => (
                <SelectItem key={user.id} value={String(user.id)}>
                  {user.email}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="space-y-1.5">
          <label
            htmlFor="add-member-role"
            className="text-xs font-bold uppercase tracking-widest text-muted-foreground"
          >
            {t('addMember.roleLabel')}
          </label>
          <Select
            value={role}
            onValueChange={(value) => {
              if (value) setRole(value as ProjectRole);
            }}
            disabled={isPending}
          >
            <SelectTrigger
              id="add-member-role"
              className="w-full"
              aria-label={t('addMember.roleAria')}
            >
              <SelectValue>
                {(value) => t(roleLabel(value as ProjectRole))}
              </SelectValue>
            </SelectTrigger>
            <SelectContent>
              {PROJECT_ROLES.map((r) => (
                <SelectItem key={r.value} value={r.value}>
                  {t(r.labelKey)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>

      <DialogFooter>
        <Button
          type="button"
          variant="outline"
          onClick={() => onOpenChange(false)}
          disabled={isPending}
        >
          {t('cancel')}
        </Button>
        <Button onClick={handleAdd} disabled={isPending || !userId}>
          {isPending ? (
            <Loader2 className="size-4 animate-spin" />
          ) : (
            <>
              <UserPlus className="mr-2 size-4" />
              {t('addMember.submit')}
            </>
          )}
        </Button>
      </DialogFooter>
    </>
  );
}
