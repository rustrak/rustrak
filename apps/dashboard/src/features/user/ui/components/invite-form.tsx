import { zodResolver } from '@hookform/resolvers/zod';
import type {
  CatalogResource,
  InvitationResponse,
  Role,
} from '@rustrak/client';
import {
  Button,
  DialogBody,
  DialogFooter,
  DialogHeader,
  Input,
  Select,
  SelectItem,
  Switch,
} from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { useForm } from 'react-hook-form';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { z } from 'zod';
import { userMutations } from '@/features/user/api/mutations';
import { reachesProjects } from '@/features/user/model/assignments';
import { copyToClipboard } from '@/shared/lib/clipboard';
import { applyServerFieldErrors } from '@/shared/lib/form-errors';
import {
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  FormRootError,
} from '@/shared/ui/components/form';

const inviteSchema = z.object({
  email: z.email('Please enter a valid email address'),
  /** `owner`, a role's id, or `none` (no access until given some). */
  access: z.string(),
  /** Where the role applies: `all`, or a project's id. */
  where: z.string(),
  sendEmail: z.boolean(),
});

const NO_ROLE = 'none';
const OWNER = 'owner';
const ALL_PROJECTS = 'all';

type InviteFormData = z.infer<typeof inviteSchema>;

const DEFAULTS: InviteFormData = {
  email: '',
  access: NO_ROLE,
  where: ALL_PROJECTS,
  sendEmail: false,
};

interface InviteFormProps {
  /**
   * Whether the server can send invitation emails. False keeps the switch
   * disabled with an explanation, so the admin learns the feature exists.
   */
  emailAvailable: boolean;
  /** Only an owner invites an owner. */
  isOwner: boolean;
  /** Roles the invitation can carry; `null` when the reader may not list them. */
  roles: Role[] | null;
  /** To tell which roles reach projects at all. */
  catalog: CatalogResource[];
  /** Where a role can be limited to. */
  projects: { id: number; name: string }[];
  /** Closes the dialog. */
  onDone: () => void;
}

/**
 * Invite someone: their email, the role they get and, when it reaches
 * projects, where. The contents of a dialog.
 */
export function InviteForm({
  emailAvailable,
  isOwner,
  roles,
  catalog,
  projects,
  onDone,
}: InviteFormProps) {
  const t = useTranslations('user');
  const globalT = useTranslations();
  const invite = useMutation(userMutations.invite());
  const isPending = invite.isPending;

  const form = useForm<InviteFormData>({
    resolver: zodResolver(inviteSchema),
    defaultValues: DEFAULTS,
  });
  const access = form.watch('access');
  const accessItems = {
    [NO_ROLE]: t('invite.noRole'),
    ...(isOwner ? { [OWNER]: t('roles.admin') } : {}),
    ...Object.fromEntries((roles ?? []).map((r) => [String(r.id), r.name])),
  };
  const chosen = roles?.find((r) => String(r.id) === access);
  const asksWhere = chosen && reachesProjects(chosen.permissions, catalog);
  const whereItems = {
    [ALL_PROJECTS]: t('invite.allProjects'),
    ...Object.fromEntries(projects.map((p) => [String(p.id), p.name])),
  };
  const accessHelp =
    access === OWNER
      ? t('invite.ownerHint')
      : chosen?.description || t('invite.accessHint');

  const onSubmit = (data: InviteFormData) => {
    const deliverByEmail = data.sendEmail && emailAvailable;
    invite.mutate(
      {
        email: data.email,
        role: data.access === OWNER ? 'admin' : 'member',
        ...(data.access === NO_ROLE || data.access === OWNER
          ? {}
          : { role_id: Number(data.access) }),
        ...(asksWhere && data.where !== ALL_PROJECTS
          ? { project_id: Number(data.where) }
          : {}),
        ...(deliverByEmail ? { send_email: true } : {}),
      },
      {
        onError: (error) => {
          // An address that already has an account, or already has a pending
          // invite, comes back as a `conflict` naming `email`. That belongs
          // on the email input, where the fix is.
          const applied = applyServerFieldErrors(form, error.error, {
            labels: {
              email: t('invite.fieldEmail'),
              role: t('invite.roleLabel'),
              role_id: t('invite.roleLabel'),
              project_id: t('invite.whereLabel'),
            },
            t: globalT,
          });
          if (applied.formLevel) {
            toast.error(t('toast.createFailed'), {
              description: applied.formLevel,
            });
          }
        },
        onSuccess: (invitation) => {
          onDone();
          void announce(invitation, deliverByEmail);
        },
      },
    );
  };

  /**
   * An emailed invitation needs nothing more from the admin. Otherwise the
   * link goes on the clipboard, and a failed send says so.
   */
  const announce = async (
    invitation: InvitationResponse,
    deliverByEmail: boolean,
  ) => {
    if (invitation.email_sent_at) {
      toast.success(t('toast.emailSent'), {
        description: t('toast.emailSentHint', { email: invitation.email }),
      });
      return;
    }
    const link = `${window.location.origin}/invite/${invitation.token}`;
    const copied = await copyToClipboard(link);
    if (deliverByEmail) {
      // The invitation exists; only the email failed.
      toast.warning(t('toast.emailFailed'), {
        description: copied
          ? t('toast.emailFailedHint')
          : t('toast.copyInviteLink', { link }),
      });
    } else if (copied) {
      toast.success(t('toast.created'), {
        description: t('toast.createdHint'),
      });
    } else {
      // Couldn't auto-copy (e.g. plain HTTP). Surface the link so it isn't
      // lost; it's also re-copyable from the pending invitations list.
      toast.success(t('toast.created'), {
        description: t('toast.copyInviteLink', { link }),
      });
    }
  };

  return (
    <Form {...form}>
      <form onSubmit={form.handleSubmit(onSubmit)}>
        <DialogHeader
          title={t('invite.title')}
          description={t('invite.description')}
        />
        <DialogBody className="flex flex-col gap-4">
          <FormField
            control={form.control}
            name="email"
            render={({ field }) => (
              <FormItem>
                <FormLabel>{t('invite.emailLabel')}</FormLabel>
                <FormControl>
                  <Input
                    type="email"
                    placeholder="name@company.com"
                    autoComplete="off"
                    autoFocus
                    disabled={isPending}
                    {...field}
                  />
                </FormControl>
                <FormMessage />
              </FormItem>
            )}
          />
          <FormField
            control={form.control}
            name="access"
            render={({ field }) => (
              <FormItem>
                <FormLabel>{t('invite.roleLabel')}</FormLabel>
                <FormControl>
                  <Select
                    value={field.value}
                    onValueChange={field.onChange}
                    disabled={isPending}
                    aria-label={t('invite.roleAria')}
                    items={accessItems}
                  >
                    {Object.entries(accessItems).map(([value, label]) => (
                      <SelectItem key={value} value={value}>
                        {label}
                      </SelectItem>
                    ))}
                  </Select>
                </FormControl>
                <FormDescription>{accessHelp}</FormDescription>
                <FormMessage />
              </FormItem>
            )}
          />
          {asksWhere ? (
            <FormField
              control={form.control}
              name="where"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t('invite.whereLabel')}</FormLabel>
                  <FormControl>
                    <Select
                      value={field.value}
                      onValueChange={field.onChange}
                      disabled={isPending}
                      aria-label={t('invite.whereLabel')}
                      items={whereItems}
                    >
                      {Object.entries(whereItems).map(([value, label]) => (
                        <SelectItem key={value} value={value}>
                          {label}
                        </SelectItem>
                      ))}
                    </Select>
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
          ) : null}
          <FormField
            control={form.control}
            name="sendEmail"
            render={({ field }) => (
              <FormItem>
                <div className="flex items-center justify-between gap-4">
                  <FormLabel>{t('invite.sendEmailLabel')}</FormLabel>
                  <FormControl>
                    <Switch
                      checked={field.value}
                      onCheckedChange={(checked) => field.onChange(checked)}
                      disabled={isPending || !emailAvailable}
                      aria-label={t('invite.sendEmailAria')}
                    />
                  </FormControl>
                </div>
                {emailAvailable ? null : (
                  <FormDescription>
                    {t('invite.sendEmailUnavailable')}
                  </FormDescription>
                )}
              </FormItem>
            )}
          />
          <FormRootError />
        </DialogBody>
        <DialogFooter>
          <Button variant="secondary" onClick={onDone} disabled={isPending}>
            {t('cancel')}
          </Button>
          <Button type="submit" variant="primary" loading={isPending}>
            {isPending ? t('invite.inviting') : t('invite.submit')}
          </Button>
        </DialogFooter>
      </form>
    </Form>
  );
}
