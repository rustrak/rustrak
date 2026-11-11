import { Button, Field, FieldLabel, Input, Text } from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslations } from 'use-intl';
import { z } from 'zod';
import { userMutations } from '@/features/user/api/mutations';
import { setServerErrors } from '@/shared/lib/form-errors';
import { revalidateLogic, useAppForm } from '@/shared/ui/components/app-form';
import { useRouter } from '@/shared/ui/hooks/use-router';

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
  const router = useRouter();
  const accept = useMutation(userMutations.acceptInvitation());
  const isPending = accept.isPending;
  const [awaitingSso, setAwaitingSso] = useState(false);

  const acceptSchema = z
    .object({
      password: z.string().min(1, t('form.passwordRequired')),
      confirmPassword: z.string().min(1, t('form.confirmRequired')),
    })
    .refine((data) => data.password === data.confirmPassword, {
      message: t('form.passwordsMismatch'),
      path: ['confirmPassword'],
    });

  const form = useAppForm({
    defaultValues: { password: '', confirmPassword: '' } as AcceptFormData,
    validationLogic: revalidateLogic(),
    validators: { onDynamic: acceptSchema },
    onSubmit: ({ value }) => {
      accept.mutate(
        { token, password: value.password },
        {
          // With password login disabled the account exists but no session was
          // opened: the way in is the provider, from the login page.
          onSuccess: (accepted) => {
            if (accepted.signed_in) router.push('/projects');
            else setAwaitingSso(true);
          },
          onError: ({ error }) =>
            setServerErrors(form, {
              fields: { confirmPassword: error.message },
            }),
        },
      );
    },
  });

  if (awaitingSso) {
    return (
      <div className="flex flex-col gap-6">
        <div className="flex flex-col gap-1">
          <h1 className="text-fg text-page-title">{t('form.createdTitle')}</h1>
          <Text render={<p />} tone="muted">
            {t('form.createdUseSso')}
          </Text>
        </div>
        <Button
          variant="primary"
          size="lg"
          className="w-full"
          onClick={() => router.push('/login')}
        >
          {t('form.goToLogin')}
        </Button>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-1">
        <h1 className="text-fg text-page-title">{t('form.title')}</h1>
        <Text render={<p />} tone="muted">
          {t('form.subtitle')}
        </Text>
      </div>

      <Field>
        <FieldLabel>{t('form.emailLabel')}</FieldLabel>
        <Input value={email} readOnly disabled />
      </Field>

      <form.AppForm>
        <form.Form className="flex flex-col gap-4">
          <form.AppField name="password">
            {(field) => (
              <field.TextField
                label={t('form.passwordLabel')}
                type="password"
                placeholder={t('form.passwordPlaceholder')}
                autoComplete="new-password"
                disabled={isPending}
              />
            )}
          </form.AppField>

          <form.AppField name="confirmPassword">
            {(field) => (
              <field.TextField
                label={t('form.confirmLabel')}
                type="password"
                placeholder={t('form.confirmPlaceholder')}
                autoComplete="new-password"
                disabled={isPending}
              />
            )}
          </form.AppField>

          <Button
            type="submit"
            variant="primary"
            size="lg"
            className="mt-2 w-full"
            loading={isPending}
          >
            {isPending ? t('form.creating') : t('form.accept')}
          </Button>
        </form.Form>
      </form.AppForm>
    </div>
  );
}
