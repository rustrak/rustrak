import { zodResolver } from '@hookform/resolvers/zod';
import { Button, Field, FieldLabel, Input, Text } from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { useForm } from 'react-hook-form';
import { useTranslations } from 'use-intl';
import { z } from 'zod';
import { userMutations } from '@/features/user/api/mutations';
import { SERVER_ERROR_PATH } from '@/shared/lib/form-errors';
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  FormRootError,
} from '@/shared/ui/components/form';
import { Link } from '@/shared/ui/components/link';
import { useRouter } from '@/shared/ui/hooks/use-router';

type LinkFormData = { password: string };

export function LinkAccountForm({
  email,
  provider,
}: {
  email: string;
  provider: string;
}) {
  const t = useTranslations('auth');
  const router = useRouter();
  const confirm = useMutation(userMutations.confirmSsoLink());
  // Still busy once it succeeded: the browser is on its way in.
  const isPending = confirm.isPending || confirm.isSuccess;

  const form = useForm<LinkFormData>({
    resolver: zodResolver(
      z.object({ password: z.string().min(1, t('link.passwordRequired')) }),
    ),
    defaultValues: { password: '' },
  });

  const onSubmit = (data: LinkFormData) => {
    form.clearErrors();

    confirm.mutate(data.password, {
      onSuccess: () => router.push('/'),
      onError: ({ error }) => {
        // Only a wrong password is a verdict on the field; everything else,
        // including an identity already linked elsewhere, goes under the form.
        if (error.kind === 'unauthenticated') {
          form.setError('password', {
            type: 'server',
            message: t('link.invalidPassword'),
          });
          return;
        }
        form.setError(SERVER_ERROR_PATH, {
          type: 'server',
          message:
            error.kind === 'conflict'
              ? t('link.alreadyLinked', { provider })
              : t('link.failure'),
        });
      },
    });
  };

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-1">
        <h1 className="text-fg text-page-title">{t('link.title')}</h1>
        <Text render={<p />} tone="muted">
          {t('link.description', { email, provider })}
        </Text>
      </div>

      <Field>
        <FieldLabel>{t('link.emailLabel')}</FieldLabel>
        <Input value={email} readOnly disabled />
      </Field>

      <Form {...form}>
        <form
          noValidate
          onSubmit={form.handleSubmit(onSubmit)}
          className="flex flex-col gap-4"
        >
          <FormField
            control={form.control}
            name="password"
            render={({ field }) => (
              <FormItem>
                <FormLabel>{t('link.passwordLabel')}</FormLabel>
                <FormControl>
                  <Input
                    type="password"
                    placeholder={t('link.passwordPlaceholder')}
                    autoComplete="current-password"
                    disabled={isPending}
                    {...field}
                  />
                </FormControl>
                <FormMessage />
              </FormItem>
            )}
          />

          <FormRootError />

          <Button
            type="submit"
            variant="primary"
            size="lg"
            className="mt-2 w-full"
            loading={isPending}
          >
            {isPending ? t('link.submitting') : t('link.submit')}
          </Button>
        </form>
      </Form>

      <Button
        variant="ghost"
        size="lg"
        className="w-full"
        render={<Link href="/login" />}
      >
        {t('link.cancel')}
      </Button>
    </div>
  );
}
