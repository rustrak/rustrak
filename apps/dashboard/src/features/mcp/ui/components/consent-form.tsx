import type { AuthorizationInfo, AuthorizationRequest } from '@rustrak/client';
import { Button, SecurityIcon } from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { mcpMutations } from '@/features/mcp/api/mutations';
import { navigableRedirect } from '@/features/mcp/lib/navigable-redirect';
import { useToastFailure } from '@/shared/ui/hooks/use-toast-failure';

interface ConsentFormProps {
  request: AuthorizationRequest;
  client: AuthorizationInfo;
  email: string;
}

/**
 * Asks the signed-in user whether an assistant may act as them.
 *
 * Both buttons go through the server, which verifies the request again and
 * answers with where to send the browser: the page never builds a redirect
 * itself, so it can only ever send the user to a URL the client registered.
 */
export function ConsentForm({ request, client, email }: ConsentFormProps) {
  const t = useTranslations('oauth');
  const toastFailure = useToastFailure();
  const decision = useMutation(mcpMutations.decideAuthorization(request));
  const isPending = decision.isPending;

  const decide = (approve: boolean) =>
    decision.mutate(approve, {
      onError: (error) => toastFailure(t('decideFailed'), error),
      onSuccess: ({ redirect_to }) => {
        if (!navigableRedirect(redirect_to)) {
          toast.error(t('decideFailed'));
          return;
        }
        window.location.assign(redirect_to);
      },
    });

  return (
    <div className="space-y-8">
      <div className="space-y-2">
        <h1 className="text-page-title text-fg">
          {t('title', { client: client.client_name })}
        </h1>
        <p className="text-body text-fg-muted">{t('signedInAs', { email })}</p>
      </div>

      <div className="space-y-3 rounded-lg border border-border-subtle bg-surface p-4 text-body text-fg">
        <p className="font-medium">{t('canDo')}</p>
        <ul className="list-disc space-y-1 pl-5 text-fg-muted">
          <li>{t('canRead')}</li>
          <li>{t('canAct')}</li>
        </ul>
      </div>

      <dl className="space-y-2 text-body">
        <div>
          <dt className="text-fg-muted">{t('clientId')}</dt>
          <dd className="break-all font-mono text-mono-sm text-fg">
            {client.client_id}
          </dd>
        </div>
        <div>
          <dt className="text-fg-muted">{t('returnsTo')}</dt>
          <dd className="font-mono text-mono-sm text-fg">
            {client.redirect_host}
          </dd>
        </div>
      </dl>

      <p className="flex gap-2 text-hint text-fg-muted">
        <SecurityIcon size="md" aria-hidden="true" className="shrink-0" />
        {t('warning')}
      </p>

      <div className="flex gap-3">
        <Button
          variant="primary"
          size="lg"
          className="flex-1"
          disabled={isPending}
          onClick={() => decide(true)}
        >
          {t('approve')}
        </Button>
        <Button
          variant="secondary"
          size="lg"
          className="flex-1"
          disabled={isPending}
          onClick={() => decide(false)}
        >
          {t('deny')}
        </Button>
      </div>
    </div>
  );
}
