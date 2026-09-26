import { zodResolver } from '@hookform/resolvers/zod';
import type { GlobalRole } from '@rustrak/client';
import { UserPlus } from 'lucide-react';
import { useTransition } from 'react';
import { useForm } from 'react-hook-form';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { z } from 'zod';
import { createInvitation } from '@/features/user/api/mutations';
import { invalidate, scope } from '@/shared/api/query-client';
import { copyToClipboard } from '@/shared/lib/clipboard';
import { applyServerFieldErrors } from '@/shared/lib/form-errors';
import { Button } from '@/shared/ui/components/shadcn/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/shared/ui/components/shadcn/card';
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  FormRootError,
} from '@/shared/ui/components/shadcn/form';
import { Input } from '@/shared/ui/components/shadcn/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/shared/ui/components/shadcn/select';

const inviteSchema = z.object({
  email: z.email('Please enter a valid email address'),
  role: z.enum(['admin', 'member']),
});

type InviteFormData = z.infer<typeof inviteSchema>;

export function InviteForm() {
  const t = useTranslations('user');

  const globalT = useTranslations();
  const [isPending, startTransition] = useTransition();

  const onSubmit = (data: InviteFormData) => {
    startTransition(async () => {
      const result = await createInvitation({
        email: data.email,
        role: data.role as GlobalRole,
      });

      if (!result.success) {
        // An address that already has an account, or already has a pending
        // invite, comes back as a `conflict` naming `email`. That belongs on
        // the email input, where the fix is.
        const applied = applyServerFieldErrors(form, result.error, {
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
        return;
      }

      const link = `${window.location.origin}/invite/${result.data.token}`;
      form.reset({ email: '', role: 'member' });
      void invalidate(scope.team);

      const copied = await copyToClipboard(link);
      if (copied) {
        toast.success(t('toast.created'), {
          description: t('toast.createdHint'),
        });
      } else {
        // Couldn't auto-copy (e.g. plain HTTP). Surface the link so it isn't lost;
        // it's also re-copyable from the pending invitations list below.
        toast.success(t('toast.created'), {
          description: t('toast.copyInviteLink', { link }),
        });
      }
    });
  };

  const form = useForm<InviteFormData>({
    resolver: zodResolver(inviteSchema),
    defaultValues: {
      email: '',
      role: 'member',
    },
  });

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('invite.title')}</CardTitle>
        <CardDescription>{t('invite.description')}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <Form {...form}>
          <form
            onSubmit={form.handleSubmit(onSubmit)}
            className="flex flex-col gap-4 sm:flex-row sm:items-start"
          >
            <FormField
              control={form.control}
              name="email"
              render={({ field }) => (
                <FormItem className="flex-1">
                  <FormLabel className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
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

            <FormField
              control={form.control}
              name="role"
              render={({ field }) => (
                <FormItem className="sm:w-40">
                  <FormLabel className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
                    {t('invite.roleLabel')}
                  </FormLabel>
                  <Select
                    value={field.value}
                    onValueChange={(value) => {
                      if (value) field.onChange(value);
                    }}
                    disabled={isPending}
                  >
                    <FormControl>
                      <SelectTrigger
                        className="w-full"
                        aria-label={t('invite.roleAria')}
                      >
                        <SelectValue>
                          {(value) =>
                            t(
                              value === 'admin'
                                ? 'roles.admin'
                                : 'roles.member',
                            )
                          }
                        </SelectValue>
                      </SelectTrigger>
                    </FormControl>
                    <SelectContent>
                      <SelectItem value="member">
                        {t('roles.member')}
                      </SelectItem>
                      <SelectItem value="admin">{t('roles.admin')}</SelectItem>
                    </SelectContent>
                  </Select>
                  <FormMessage />
                </FormItem>
              )}
            />

            <Button
              type="submit"
              disabled={isPending}
              className="sm:mt-[26px] font-bold uppercase tracking-wider"
            >
              <UserPlus className="mr-2 size-4" />
              {isPending ? t('invite.inviting') : t('invite.submit')}
            </Button>
          </form>
          {/* Where a failure that named no field of this form lands. */}
          <FormRootError />
        </Form>
      </CardContent>
    </Card>
  );
}
