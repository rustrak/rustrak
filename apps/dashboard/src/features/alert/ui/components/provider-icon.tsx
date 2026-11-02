import {
  HashIcon,
  MailIcon,
  NotificationIcon,
  PayloadIcon,
  WebhookIcon,
} from '@rustrak/ui';

/**
 * The mark for an alert provider.
 *
 * `NotificationIcon` is the fallback rather than a missing glyph: the server may name a
 * provider this build has never heard of, and a row with no icon reads as a
 * broken row instead of an unfamiliar one.
 */
export function ProviderIcon({
  type,
  className,
}: {
  type: string;
  className?: string;
}) {
  switch (type) {
    case 'slack':
      return <HashIcon className={className} />;
    case 'email':
      return <MailIcon className={className} />;
    case 'webhook':
      return <WebhookIcon className={className} />;
    case 'custom_webhook':
      return <PayloadIcon className={className} />;
    default:
      return <NotificationIcon className={className} />;
  }
}
