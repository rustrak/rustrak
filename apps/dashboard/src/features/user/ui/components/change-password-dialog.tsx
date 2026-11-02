import { zodResolver } from '@hookform/resolvers/zod';
import {
  Button,
  Dialog,
  DialogBody,
  DialogClose,
  DialogFooter,
  DialogHeader,
} from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { useForm } from 'react-hook-form';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { z } from 'zod';
import { userMutations } from '@/features/user/api/mutations';
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
import { PasswordInput } from '@/shared/ui/components/password-input';

type ChangePasswordFormData = {
  current_password: string;
  new_password: string;
  confirm_password: string;
};

const EMPTY: ChangePasswordFormData = {
  current_password: '',
  new_password: '',
  confirm_password: '',
};

const FIELDS = [
  ['current_password', 'current', 'current-password'],
  ['new_password', 'new', 'new-password'],
  ['confirm_password', 'confirm', 'new-password'],
] as const;

/** The "Change password" button and the dialog it opens. */
export function ChangePasswordDialog() {
  const t = useTranslations('settings');
  const globalT = useTranslations();
  const [open, setOpen] = useState(false);
  const change = useMutation(userMutations.changePassword());
  const isPending = change.isPending;

  const form = useForm<ChangePasswordFormData>({
    resolver: zodResolver(
      z
        .object({
          current_password: z
            .string()
            .min(1, t('account.password.currentRequired')),
          new_password: z.string().min(1, t('account.password.newRequired')),
          confirm_password: z.string(),
        })
        .refine((data) => data.new_password === data.confirm_password, {
          path: ['confirm_password'],
          message: t('account.password.mismatch'),
        }),
    ),
    defaultValues: EMPTY,
  });

  // Typed passwords must not survive the dialog closing, whichever way it closes.
  const close = () => {
    form.reset(EMPTY);
    setOpen(false);
  };

  // Stays open while a request is in flight, so its outcome cannot land on a
  // later opening of the dialog.
  const onOpenChange = (next: boolean) => {
    if (next) setOpen(true);
    else if (!isPending) close();
  };

  const onSubmit = (data: ChangePasswordFormData) => {
    change.mutate(
      {
        current_password: data.current_password,
        new_password: data.new_password,
      },
      {
        onSuccess: () => {
          close();
          toast.success(t('account.password.changed'));
        },
        onError: (error) =>
          applyServerFieldErrors(form, error.error, {
            labels: {
              current_password: t('account.password.current'),
              new_password: t('account.password.new'),
            },
            t: globalT,
          }),
      },
    );
  };

  return (
    <>
      <Button onClick={() => setOpen(true)}>
        {t('account.password.change')}
      </Button>
      <Dialog open={open} onOpenChange={onOpenChange} size="md">
        <DialogHeader
          title={t('account.password.change')}
          description={t('account.password.dialogDescription')}
        />

        <Form {...form}>
          <form
            noValidate
            onSubmit={form.handleSubmit(onSubmit)}
            className="flex min-h-0 flex-col"
          >
            <DialogBody className="flex flex-col gap-4">
              {FIELDS.map(([name, label, autoComplete]) => (
                <FormField
                  key={name}
                  control={form.control}
                  name={name}
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>{t(`account.password.${label}`)}</FormLabel>
                      <FormControl>
                        <PasswordInput
                          autoComplete={autoComplete}
                          disabled={isPending}
                          {...field}
                        />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
              ))}

              {/* Where a failure that named no field of this form lands. */}
              <FormRootError />
            </DialogBody>

            <DialogFooter>
              <DialogClose
                disabled={isPending}
                render={
                  <Button variant="secondary">
                    {t('account.password.cancel')}
                  </Button>
                }
              />
              <Button type="submit" variant="primary" loading={isPending}>
                {isPending
                  ? t('account.password.submitting')
                  : t('account.password.submit')}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </Dialog>
    </>
  );
}
