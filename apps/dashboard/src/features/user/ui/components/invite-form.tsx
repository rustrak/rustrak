import { zodResolver } from '@hookform/resolvers/zod';
import type { InvitationResponse, UserRole } from '@rustrak/client';
import {
  Button,
  Input,
  NewIcon,
  Select,
  SelectItem,
  Switch,
} from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { useId } from 'react';
import { useForm } from 'react-hook-form';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { z } from 'zod';
import { userMutations } from '@/features/user/api/mutations';
import { copyToClipboard } from '@/shared/lib/clipboard';
import { applyServerFieldErrors } from '@/shared/lib/form-errors';
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  FormRootError,
} from '@/shared/ui/components/form';
import { SettingRow, SettingSection } from '@/shared/ui/components/setting-row';

const inviteSchema = z.object({
  email: z.email('Please enter a valid email address'),
  role: z.enum(['admin', 'member']),
  sendEmail: z.boolean(),
});

type InviteFormData = z.infer<typeof inviteSchema>;

interface InviteFormProps {
  /**
   * Whether the server can send invitation emails. False keeps the switch
   * disabled with an explanation, so the admin learns the feature exists.
   */
  emailAvailable: boolean;
}

/**
 * A section of two rows, with the submit button beside its heading. The
 * button sits outside the `<form>` and joins it through the `form`
 * attribute, so Enter in the email field still submits.
 */
export function InviteForm({ emailAvailable }: InviteFormProps) {
  const t = useTranslations('user');
  const formId = useId();

  const globalT = useTranslations();
  const invite = useMutation(userMutations.invite());
  const isPending = invite.isPending;

  const onSubmit = (data: InviteFormData) => {
    const deliverByEmail = data.sendEmail && emailAvailable;
    invite.mutate(
      {
        email: data.email,
        role: data.role as UserRole,
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
            },
            t: globalT,
          });

          if (applied.formLevel) {
            toast.error(t('toast.createFailed'), {
              description: applied.formLevel,
            });
          }
        },
        onSuccess: (invitation) => copyInvite(invitation, deliverByEmail),
      },
    );
  };

  /**
   * An emailed invitation needs nothing more from the admin. Otherwise the
   * link goes on the clipboard, and a failed send says so.
   */
  const copyInvite = async (
    invitation: InvitationResponse,
    deliverByEmail: boolean,
  ) => {
    const link = `${window.location.origin}/invite/${invitation.token}`;
    form.reset({ email: '', role: 'member', sendEmail: false });

    if (invitation.email_sent_at) {
      toast.success(t('toast.emailSent'), {
        description: t('toast.emailSentHint', { email: invitation.email }),
      });
      return;
    }

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
      // lost; it's also re-copyable from the pending invitations list below.
      toast.success(t('toast.created'), {
        description: t('toast.copyInviteLink', { link }),
      });
    }
  };

  const form = useForm<InviteFormData>({
    resolver: zodResolver(inviteSchema),
    defaultValues: {
      email: '',
      role: 'member',
      sendEmail: false,
    },
  });

  return (
    <SettingSection
      title={t('invite.title')}
      description={t('invite.description')}
      actions={
        <Button
          type="submit"
          form={formId}
          variant="primary"
          icon={NewIcon}
          loading={isPending}
        >
          {isPending ? t('invite.inviting') : t('invite.submit')}
        </Button>
      }
    >
      <Form {...form}>
        <form id={formId} onSubmit={form.handleSubmit(onSubmit)}>
          <SettingRow title={t('invite.emailLabel')}>
            <FormField
              control={form.control}
              name="email"
              render={({ field }) => (
                <FormItem>
                  <FormLabel className="sr-only">
                    {t('invite.emailLabel')}
                  </FormLabel>
                  <FormControl>
                    <Input
                      type="email"
                      placeholder="name@company.com"
                      autoComplete="off"
                      disabled={isPending}
                      {...field}
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
          </SettingRow>

          <SettingRow title={t('invite.roleLabel')}>
            <FormField
              control={form.control}
              name="role"
              render={({ field, fieldState }) => (
                <FormItem>
                  <FormControl>
                    <Select<InviteFormData['role']>
                      value={field.value}
                      onValueChange={field.onChange}
                      disabled={isPending}
                      invalid={!!fieldState.error}
                      aria-label={t('invite.roleAria')}
                      items={{
                        member: t('roles.member'),
                        admin: t('roles.admin'),
                      }}
                    >
                      <SelectItem value="member">
                        {t('roles.member')}
                      </SelectItem>
                      <SelectItem value="admin">{t('roles.admin')}</SelectItem>
                    </Select>
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
          </SettingRow>

          <SettingRow
            title={t('invite.sendEmailLabel')}
            description={
              emailAvailable ? undefined : t('invite.sendEmailUnavailable')
            }
          >
            <FormField
              control={form.control}
              name="sendEmail"
              render={({ field }) => (
                <FormItem>
                  <FormControl>
                    <Switch
                      checked={field.value}
                      onCheckedChange={(checked) => field.onChange(checked)}
                      disabled={isPending || !emailAvailable}
                      aria-label={t('invite.sendEmailAria')}
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
          </SettingRow>
        </form>
        {/* Where a failure that named no field of this form lands. */}
        <FormRootError className="px-5 pb-4" />
      </Form>
    </SettingSection>
  );
}
