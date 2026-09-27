import { zodResolver } from '@hookform/resolvers/zod';
import { useNavigate } from '@tanstack/react-router';
import { useTransition } from 'react';
import { useForm } from 'react-hook-form';
import { useTranslations } from 'use-intl';
import { z } from 'zod';
import { acceptInvitation } from '@/features/user/api/mutations';
import { Button } from '@/shared/ui/components/shadcn/button';
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/shared/ui/components/shadcn/form';
import { Input } from '@/shared/ui/components/shadcn/input';
import { Label } from '@/shared/ui/components/shadcn/label';

type AcceptFormData = {
  password: string;
  confirmPassword: string;
};

interface AcceptInvitationFormProps {
  token: string;
  email: string;
}

export function AcceptInvitationForm({
  token,
  email,
}: AcceptInvitationFormProps) {
  const t = useTranslations('invite');
  const navigate = useNavigate();
  const [isPending, startTransition] = useTransition();

  const acceptSchema = z
    .object({
      password: z.string().min(1, t('form.passwordRequired')),
      confirmPassword: z.string().min(1, t('form.confirmRequired')),
    })
    .refine((data) => data.password === data.confirmPassword, {
      message: t('form.passwordsMismatch'),
      path: ['confirmPassword'],
    });

  const form = useForm<AcceptFormData>({
    resolver: zodResolver(acceptSchema),
    defaultValues: {
      password: '',
      confirmPassword: '',
    },
  });

  const onSubmit = (data: AcceptFormData) => {
    form.clearErrors();

    startTransition(async () => {
      const result = await acceptInvitation({ token, password: data.password });

      if (result.success) {
        navigate({ to: '/projects' });
      } else {
        form.setError('confirmPassword', {
          type: 'server',
          message: result.error.message,
        });
      }
    });
  };

  return (
    <div className="space-y-6">
      <div className="space-y-2">
        <h1 className="text-3xl font-bold tracking-tight">{t('form.title')}</h1>
        <p className="text-muted-foreground">{t('form.subtitle')}</p>
      </div>

      <div className="space-y-2">
        <Label className="text-[11px] font-bold uppercase tracking-widest text-muted-foreground">
          {t('form.emailLabel')}
        </Label>
        <Input value={email} readOnly disabled className="bg-background" />
      </div>

      <Form {...form}>
        <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
          <FormField
            control={form.control}
            name="password"
            render={({ field }) => (
              <FormItem className="space-y-2">
                <FormLabel className="text-[11px] font-bold uppercase tracking-widest text-muted-foreground">
                  {t('form.passwordLabel')}
                </FormLabel>
                <FormControl>
                  <Input
                    type="password"
                    placeholder={t('form.passwordPlaceholder')}
                    autoComplete="new-password"
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
            name="confirmPassword"
            render={({ field }) => (
              <FormItem className="space-y-2">
                <FormLabel className="text-[11px] font-bold uppercase tracking-widest text-muted-foreground">
                  {t('form.confirmLabel')}
                </FormLabel>
                <FormControl>
                  <Input
                    type="password"
                    placeholder={t('form.confirmPlaceholder')}
                    autoComplete="new-password"
                    disabled={isPending}
                    {...field}
                  />
                </FormControl>
                <FormMessage />
              </FormItem>
            )}
          />

          <Button
            type="submit"
            className="w-full font-extrabold uppercase tracking-widest text-xs py-6 mt-2"
            disabled={isPending}
          >
            {isPending ? t('form.creating') : t('form.accept')}
          </Button>
        </form>
      </Form>
    </div>
  );
}
