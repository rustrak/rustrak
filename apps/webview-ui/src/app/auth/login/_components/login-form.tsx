'use client';

import { zodResolver } from '@hookform/resolvers/zod';
import type { RustrakError, SsoConfig } from '@rustrak/client';
import { Eye, EyeOff } from 'lucide-react';
import { useRouter } from 'next/navigation';
import { useTranslations } from 'next-intl';
import { useState, useTransition } from 'react';
import { useForm } from 'react-hook-form';
import { z } from 'zod';
import { login, startSso } from '@/features/user/api/mutations';
import { SERVER_ERROR_PATH } from '@/shared/lib/form-errors';
import { Button } from '@/shared/ui/components/shadcn/button';
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

function formatWait(seconds: number, t: LoginTranslator): string {
  if (seconds < 60) return t('form.waitSeconds', { count: seconds });
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return t('form.waitMinutes', { count: minutes });
  const hours = Math.round(seconds / 3600);
  return t('form.waitHours', { count: hours });
}

export function LoginForm({
  ssoConfig,
  ssoFailed,
}: {
  ssoConfig: SsoConfig | null;
  ssoFailed: boolean;
}) {
  const t = useTranslations('auth');
  const router = useRouter();
  const [isPending, startTransition] = useTransition();
  const [isSsoPending, startSsoTransition] = useTransition();
  const [showPassword, setShowPassword] = useState(false);
  const [hasSsoError, setHasSsoError] = useState(ssoFailed);

  const loginSchema = z.object({
    email: z.email(t('form.emailInvalid')),
    password: z.string().min(1, t('form.passwordRequired')),
  });

  const form = useForm<LoginFormData>({
    resolver: zodResolver(loginSchema),
    defaultValues: {
      email: '',
      password: '',
    },
  });

  const onSubmit = (data: LoginFormData) => {
    form.clearErrors();

    startTransition(async () => {
      const result = await login(data);

      if (result.success) {
        router.push('/');
        return;
      }

      if (result.error.kind === 'unauthenticated') {
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
        form.setError('password', {
          type: 'server',
          message: t('form.invalidCredentials'),
        });
        return;
      }

      // Not a verdict on the credentials, so it does not go on a credential
      // field. "An unexpected error occurred" under the password box, when the
      // API is simply down, sends the user off to retype a password that was
      // right all along.
      //
      // The path is imported rather than written out: `FormRootError` reads
      // the same key, so a literal here would be a third copy of a constant
      // nothing checks.
      form.setError(SERVER_ERROR_PATH, {
        type: 'server',
        message: loginFailureMessage(result.error, t),
      });
    });
  };

  const onSsoLogin = () => {
    setHasSsoError(false);
    startSsoTransition(async () => {
      const result = await startSso();
      if (!result.success) {
        setHasSsoError(true);
        return;
      }

      window.location.assign(result.data);
    });
  };

  return (
    <div className="space-y-6">
      <div className="space-y-2">
        <h1 className="text-3xl font-bold tracking-tight">{t('form.title')}</h1>
        <p className="text-muted-foreground">{t('form.subtitle')}</p>
      </div>

      {ssoConfig?.enabled && (
        <>
          <div className="space-y-2">
            <Button
              type="button"
              variant="outline"
              className="w-full font-extrabold uppercase tracking-widest text-xs py-6"
              disabled={isSsoPending || isPending}
              onClick={onSsoLogin}
            >
              {isSsoPending
                ? t('form.ssoRedirecting')
                : t('form.continueWithSso', {
                    provider: ssoConfig.provider_name ?? 'SSO',
                  })}
            </Button>
            {hasSsoError && (
              <p className="text-sm font-medium text-destructive" role="alert">
                {t('form.ssoFailure')}
              </p>
            )}
          </div>

          <div className="flex items-center gap-3" aria-hidden="true">
            <div className="h-px flex-1 bg-border" />
            <span className="text-xs uppercase tracking-widest text-muted-foreground">
              {t('form.or')}
            </span>
            <div className="h-px flex-1 bg-border" />
          </div>
        </>
      )}

      <Form {...form}>
        <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
          <FormField
            control={form.control}
            name="email"
            render={({ field }) => (
              <FormItem className="space-y-2">
                <FormLabel className="text-[11px] font-bold uppercase tracking-widest text-muted-foreground">
                  {t('form.emailLabel')}
                </FormLabel>
                <FormControl>
                  <Input
                    type="email"
                    placeholder={t('form.emailPlaceholder')}
                    autoComplete="email"
                    disabled={isPending}
                    className="bg-background border-border px-4 py-3.5 text-sm placeholder:text-muted-foreground/30"
                    {...field}
                  />
                </FormControl>
                <FormMessage />
              </FormItem>
            )}
          />

          <FormField
            control={form.control}
            name="password"
            render={({ field }) => (
              <FormItem className="space-y-2">
                <div className="flex items-center justify-between">
                  <FormLabel className="text-[11px] font-bold uppercase tracking-widest text-muted-foreground">
                    {t('form.passwordLabel')}
                  </FormLabel>
                </div>
                <FormControl>
                  <div className="relative">
                    <input
                      type={showPassword ? 'text' : 'password'}
                      placeholder={t('form.passwordPlaceholder')}
                      autoComplete="current-password"
                      disabled={isPending}
                      className="h-9 w-full min-w-0 rounded-md border border-input bg-transparent px-4 py-3.5 pr-10 text-sm shadow-xs outline-none placeholder:text-muted-foreground/30 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50 dark:bg-input/30"
                      {...field}
                    />
                    <button
                      type="button"
                      onClick={() => setShowPassword((prev) => !prev)}
                      disabled={isPending}
                      className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50"
                      tabIndex={-1}
                      aria-label={
                        showPassword
                          ? t('form.hidePassword')
                          : t('form.showPassword')
                      }
                    >
                      {showPassword ? (
                        <EyeOff className="size-4" />
                      ) : (
                        <Eye className="size-4" />
                      )}
                    </button>
                  </div>
                </FormControl>
                <FormMessage />
              </FormItem>
            )}
          />

          {/* Where an outage lands, kept off the credential fields. */}
          <FormRootError />

          <Button
            type="submit"
            className="w-full font-extrabold uppercase tracking-widest text-xs py-6 mt-2"
            disabled={isPending}
          >
            {isPending ? t('form.signingIn') : t('form.login')}
          </Button>
        </form>
      </Form>
    </div>
  );
}
