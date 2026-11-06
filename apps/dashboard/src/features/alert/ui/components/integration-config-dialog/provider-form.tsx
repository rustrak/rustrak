import type { AlertIntegration, ProviderType } from '@rustrak/client';
import { CustomWebhookForm } from './forms/custom-webhook-form';
import { EmailForm } from './forms/email-form';
import { SlackForm } from './forms/slack-form';
import { WebhookForm } from './forms/webhook-form';

export interface ConfigFormProps {
  /** Called with `false` when the form is done: saved, deleted or cancelled. */
  onOpenChange: (open: boolean) => void;
  existingIntegration: AlertIntegration | null;
  onTest: (
    integration: AlertIntegration,
    routingOverride?: Record<string, unknown>,
  ) => void;
  /** Absent without `integrations:delete`. */
  onDelete?: (integration: AlertIntegration) => void;
  isPending: boolean;
}

/** The form for one provider, on that provider's settings page. */
export function ProviderForm({
  provider,
  ...props
}: { provider: ProviderType } & ConfigFormProps) {
  switch (provider) {
    case 'slack':
      return <SlackForm {...props} />;
    case 'email':
      return <EmailForm {...props} />;
    case 'webhook':
      return <WebhookForm {...props} />;
    case 'custom_webhook':
      return <CustomWebhookForm {...props} />;
    default:
      // A provider this build cannot configure.
      return null;
  }
}
