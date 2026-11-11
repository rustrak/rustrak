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
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { z } from 'zod';
import { userMutations } from '@/features/user/api/mutations';
import { applyServerFieldErrors } from '@/shared/lib/form-errors';
import { revalidateLogic, useAppForm } from '@/shared/ui/components/app-form';

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

  const form = useAppForm({
    defaultValues: EMPTY,
    validationLogic: revalidateLogic(),
    validators: {
      onDynamic: z
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
    },
    onSubmit: ({ value }) => onSubmit(value),
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

  function onSubmit(data: ChangePasswordFormData) {
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
            messages: {
              current_password: {
                incorrect: t('account.password.incorrect'),
              },
            },
            t: globalT,
          }),
      },
    );
  }

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

        <form.AppForm>
          <form.Form className="flex min-h-0 flex-col">
            <DialogBody className="flex flex-col gap-4">
              {FIELDS.map(([name, label, autoComplete]) => (
                <form.AppField key={name} name={name}>
                  {(field) => (
                    <field.PasswordField
                      label={t(`account.password.${label}`)}
                      autoComplete={autoComplete}
                      disabled={isPending}
                    />
                  )}
                </form.AppField>
              ))}

              <form.RootError />
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
          </form.Form>
        </form.AppForm>
      </Dialog>
    </>
  );
}
