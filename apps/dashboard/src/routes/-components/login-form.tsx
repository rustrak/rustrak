import type { RustrakError, SsoConfigResponse } from '@rustrak/client';
import {
  Button,
  ConcealIcon,
  InputAction,
  RevealIcon,
  Separator,
  Text,
} from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslations } from 'use-intl';
import { z } from 'zod';
import { userMutations } from '@/features/user/api/mutations';
import { setServerErrors } from '@/shared/lib/form-errors';
import { revalidateLogic, useAppForm } from '@/shared/ui/components/app-form';
import { useRouter } from '@/shared/ui/hooks/use-router';

type LoginFormData = {
  email: string;
  password: string;
};

type LoginTranslator = ReturnType<typeof useTranslations<'auth'>>;

/**
 * Copy for a failure that is not a verdict on the credentials.
 *
 * `rate_limited` is called out because the generic sentence ends "Please try
 * again", and on a login page behind a proxy rate-limit that is the one
 * instruction that makes things worse: every retry extends the block.
 */
function loginFailureMessage(error: RustrakError, t: LoginTranslator): string {
  switch (error.kind) {
    case 'network':
      return t('form.failureNetwork');
    case 'forbidden':
      // The only 403 this form can get is the SSO-only switch: the account
      // exists but may not sign in with a password.
      return t('form.passwordLoginDisallowed');
    case 'rate_limited':
      // `retryAfter` is reachable only on this arm, which is what branching on
      // the client's own union buys over a flattened domain enum.
      return error.retryAfter && error.retryAfter > 0
        ? t('form.failureRateLimited', {
            duration: formatWait(error.retryAfter, t),
          })
        : t('form.failureRateLimitedGeneric');
    default:
      return t('form.failureUnexpected');
  }
}

/** Still busy once it succeeded: the browser is on its way out. */
function stillBusy(mutation: { isPending: boolean; isSuccess: boolean }) {
  return mutation.isPending || mutation.isSuccess;
}

/** Where a failed sign-in is shown on the form, and what it says. */
function loginFailure(
  error: RustrakError,
  t: LoginTranslator,
): { form?: string; fields?: Record<string, string> } {
  if (error.kind === 'unauthenticated') {
    // **This form deliberately gets no per-field errors, and that is not an
    // oversight the field-error work should "fix".**
    //
    // Every other form in the app now reads `error.fields` and marks the
    // input the server named. Login must not, because here the server's
    // answer is itself the secret. "This email exists but the password is
    // wrong" is user enumeration: it turns the login page into an oracle
    // that confirms whether an address has an account, which is the first
    // step of a credential-stuffing run and, on a self-hosted instance, of
    // working out who the team is.
    //
    // So all three real outcomes -- unknown address, wrong password,
    // disabled account -- collapse into this one sentence, and it is
    // attached to `password` only so it lands next to the field the user
    // will retype. The server helps by making them indistinguishable
    // upstream too: it checks `is_active` *before* verifying the password,
    // so a distinct "account disabled" answer would leak the same fact.
    return { fields: { password: t('form.invalidCredentials') } };
  }

  // Not a verdict on the credentials, so it does not go on a credential
  // field. "An unexpected error occurred" under the password box, when the
  // API is simply down, sends the user off to retype a password that was
  // right all along.
  return { form: loginFailureMessage(error, t) };
}

function formatWait(seconds: number, t: LoginTranslator): string {
  if (seconds < 60) return t('form.waitSeconds', { count: seconds });
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return t('form.waitMinutes', { count: minutes });
  const hours = Math.round(seconds / 3600);
  return t('form.waitHours', { count: hours });
}

/**
 * The note shown when the operator turned password sign-in off, and the one
 * generic way back to the form: the account that may still use a password is
 * the first one, and naming it would publish which account that is.
 */
function PasswordLoginDisabled({
  provider,
  revealed,
  onReveal,
}: {
  provider: string;
  revealed: boolean;
  onReveal: () => void;
}) {
  const t = useTranslations('auth');

  return (
    <>
      <Text render={<p />} tone="muted">
        {t('form.passwordLoginDisabled', { provider })}
      </Text>
      {!revealed && (
        <Button
          type="button"
          variant="ghost"
          size="lg"
          className="w-full"
          onClick={onReveal}
        >
          {t('form.usePassword')}
        </Button>
      )}
    </>
  );
}

export function LoginForm({
  ssoConfig,
  ssoFailed,
}: {
  ssoConfig: SsoConfigResponse | null;
  ssoFailed: boolean;
}) {
  const t = useTranslations('auth');
  const router = useRouter();
  const login = useMutation(userMutations.login());
  const sso = useMutation(userMutations.startSso());
  const isPending = stillBusy(login);
  const isSsoPending = stillBusy(sso);
  const busy = isPending || isSsoPending;
  const [showPassword, setShowPassword] = useState(false);
  // Not `useState(ssoFailed)`: a callback failure arrives as `?error=sso`, so
  // the prop is the source of truth and copying it into state would freeze it
  // at the value the first render saw. These two flags only record what the
  // reader did about it -- retried, or hit a failure while starting.
  const [ssoErrorDismissed, setSsoErrorDismissed] = useState(false);
  const hasSsoError = sso.isError || (ssoFailed && !ssoErrorDismissed);
  // A failed or absent config lookup means the login page is all we have, so
  // the password form stays; the server decides whether it is allowed.
  const passwordLoginEnabled = ssoConfig?.password_login_enabled ?? true;
  // The switch hides the form for everyone, but the first account can still
  // sign in with its password as a break-glass when the provider is down. That
  // account cannot be named here without publishing which one it is, so the
  // recovery is one generic link, and the server rejects everyone else.
  const [passwordFormRevealed, setPasswordFormRevealed] = useState(false);
  const passwordFormVisible = passwordLoginEnabled || passwordFormRevealed;
  const providerName = ssoConfig?.provider_name ?? 'SSO';

  const loginSchema = z.object({
    email: z.email(t('form.emailInvalid')),
    password: z.string().min(1, t('form.passwordRequired')),
  });

  const form = useAppForm({
    defaultValues: { email: '', password: '' } as LoginFormData,
    validationLogic: revalidateLogic(),
    validators: { onDynamic: loginSchema },
    onSubmit: ({ value }) => {
      if (busy) return;

      login.mutate(value, {
        onSuccess: () => router.push('/'),
        onError: ({ error }) => setServerErrors(form, loginFailure(error, t)),
      });
    },
  });

  const onSsoLogin = () => {
    setSsoErrorDismissed(true);
    sso.mutate(undefined, {
      onSuccess: (url) => window.location.assign(url),
    });
  };

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-1">
        <h1 className="text-fg text-page-title">{t('form.title')}</h1>
        <Text render={<p />} tone="muted">
          {t('form.subtitle')}
        </Text>
      </div>

      {ssoConfig?.enabled && (
        <>
          <div className="flex flex-col gap-2">
            <Button
              type="button"
              size="lg"
              className="w-full"
              loading={isSsoPending}
              disabled={busy}
              onClick={onSsoLogin}
            >
              {isSsoPending
                ? t('form.ssoRedirecting')
                : t('form.continueWithSso', { provider: providerName })}
            </Button>
            {hasSsoError && (
              <Text variant="hint" tone="error" render={<p role="alert" />}>
                {t('form.ssoFailure')}
              </Text>
            )}
          </div>

          {passwordFormVisible && (
            <div className="flex items-center gap-3" aria-hidden="true">
              <Separator className="flex-1" />
              <Text variant="meta" tone="subtle">
                {t('form.or')}
              </Text>
              <Separator className="flex-1" />
            </div>
          )}
        </>
      )}

      {!passwordLoginEnabled && (
        <PasswordLoginDisabled
          provider={providerName}
          revealed={passwordFormRevealed}
          onReveal={() => setPasswordFormRevealed(true)}
        />
      )}

      {passwordFormVisible && (
        <form.AppForm>
          <form.Form className="flex flex-col gap-4">
            <form.AppField name="email">
              {(field) => (
                <field.TextField
                  label={t('form.emailLabel')}
                  type="email"
                  placeholder={t('form.emailPlaceholder')}
                  autoComplete="email"
                  autoFocus
                  disabled={busy}
                />
              )}
            </form.AppField>

            <form.AppField name="password">
              {(field) => (
                <field.TextField
                  label={t('form.passwordLabel')}
                  type={showPassword ? 'text' : 'password'}
                  placeholder={t('form.passwordPlaceholder')}
                  autoComplete="current-password"
                  disabled={busy}
                  action={
                    <InputAction
                      icon={showPassword ? ConcealIcon : RevealIcon}
                      tabIndex={-1}
                      onClick={() => setShowPassword((prev) => !prev)}
                      aria-label={
                        showPassword
                          ? t('form.hidePassword')
                          : t('form.showPassword')
                      }
                    />
                  }
                />
              )}
            </form.AppField>

            <form.RootError />

            <Button
              type="submit"
              variant="primary"
              size="lg"
              className="mt-2 w-full"
              loading={isPending}
              disabled={busy}
            >
              {isPending ? t('form.signingIn') : t('form.login')}
            </Button>
          </form.Form>
        </form.AppForm>
      )}
    </div>
  );
}
