import {
  FeedbackIcon,
  IssuesIcon,
  NotificationIcon,
  UnresolveIcon,
} from '@rustrak/ui';

/**
 * The mark for an alert trigger.
 *
 * Same shape as `ProviderIcon`, and for the same reason: the type is a string
 * off the wire, so an unknown one has to render something.
 */
export function AlertTypeIcon({
  type,
  className,
}: {
  type: string;
  className?: string;
}) {
  switch (type) {
    case 'new_issue':
      return <IssuesIcon className={className} />;
    case 'regression':
      return <UnresolveIcon className={className} />;
    case 'new_feedback':
      return <FeedbackIcon className={className} />;
    default:
      return <NotificationIcon className={className} />;
  }
}
