import { createFileRoute, redirect } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { getLastEvent } from '@/features/event/api/queries';
import { issueQueries } from '@/features/issue/api/queries';
import { LoadFailure } from '@/shared/ui/components/load-failure';

/**
 * Issue page that redirects to the last event.
 * Viewing an issue immediately shows the most recent event.
 *
 * In `beforeLoad`, so the redirect happens before anything paints. It still
 * has a component, for the two failures that are not a redirect: a failed read
 * must not fall through to the empty-state route, because "this issue has no
 * events" is a very different claim from "we could not ask".
 */
export const Route = createFileRoute(
  '/_authenticated/projects/$id/issues/$issueId/',
)({
  beforeLoad: async ({ params: { id, issueId }, context: { queryClient } }) => {
    // Both at once, and the issue into the cache the event page reads it from.
    const [issue, lastEvent] = await Promise.all([
      queryClient.ensureQueryData(issueQueries.detail(id, issueId)),
      getLastEvent(id, issueId),
    ]);

    if (!issue.success) {
      return {
        failure: { error: issue.error, title: 'loadIssueFailed' as const },
      };
    }

    if (!lastEvent.success) {
      return {
        failure: {
          error: lastEvent.error,
          title: 'issues.loadLatestEventFailed' as const,
        },
      };
    }

    if (lastEvent.data) {
      throw redirect({
        to: '/projects/$id/issues/$issueId/events/$eventId',
        params: { id, issueId, eventId: lastEvent.data.id },
      });
    }

    throw redirect({
      to: '/projects/$id/issues/$issueId/events/empty',
      params: { id, issueId },
    });
  },
  component: IssuePage,
});

function IssuePage() {
  const t = useTranslations('projectPages');
  const { failure } = Route.useRouteContext();

  if (!failure) return null;

  return <LoadFailure error={failure.error} title={t(failure.title)} />;
}
