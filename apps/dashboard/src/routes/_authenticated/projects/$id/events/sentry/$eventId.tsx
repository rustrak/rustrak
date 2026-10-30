import { noop, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, Navigate } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { eventQueries } from '@/features/event/api/queries';
import { EventLoading } from '@/features/event/ui/components/event-loading';

/**
 * An event addressed by the id its SDK reported, as a feedback names the
 * error it was about. Redirects to that event inside its issue.
 *
 * Like the issue page: the event view's skeleton until the event is known,
 * then a redirect, so the navigation commits at once. The page itself only
 * renders for an event with no issue.
 */
export const Route = createFileRoute(
  '/_authenticated/projects/$id/events/sentry/$eventId',
)({
  loader: ({ context: { queryClient }, params }) => {
    void queryClient
      .query(
        eventQueries.bySentryId(Number.parseInt(params.id, 10), params.eventId),
      )
      .catch(noop);
  },
  pendingComponent: EventLoading,
  component: SentryEventPage,
});

function SentryEventPage() {
  const t = useTranslations('projectPages');
  const { id, eventId } = Route.useParams();
  const { data: event } = useSuspenseQuery(
    eventQueries.bySentryId(Number.parseInt(id, 10), eventId),
  );

  if (event.issue_id) {
    return (
      <Navigate
        replace
        to="/projects/$id/issues/$issueId/events/$eventId"
        params={{
          id,
          issueId: String(event.issue_id),
          eventId: String(event.id),
        }}
      />
    );
  }

  return (
    <p className="p-8 text-sm text-muted-foreground">
      {t('event.feedback.errorNotFound')}
    </p>
  );
}
