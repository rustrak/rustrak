import { noop, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, Navigate } from '@tanstack/react-router';
import { eventQueries } from '@/features/event/api/queries';
import { EventLoading } from '@/features/event/ui/components/event-loading';
import { issueQueries } from '@/features/issue/api/queries';

/**
 * An issue opens on its most recent event.
 *
 * The page is the event view's skeleton until the latest event is known, then
 * a redirect to it: the navigation commits at once instead of waiting on the
 * read. A failed read lands in the error boundary rather than on the
 * empty-state route, because "this issue has no events" is a very different
 * claim from "we could not ask".
 */
export const Route = createFileRoute(
  '/_authenticated/projects/$id/issues/$issueId/',
)({
  loader: ({ context: { queryClient }, params }) => {
    const projectId = Number.parseInt(params.id, 10);
    const latest = eventQueries.latest(projectId, params.issueId);
    // Asked every time: a cached answer would open an older event than the
    // one that just brought the reader here.
    queryClient.removeQueries({ queryKey: latest.queryKey, exact: true });
    void queryClient.query(latest).catch(noop);
    // The event page reads it next; on its way already.
    void queryClient
      .query(issueQueries.detail(projectId, params.issueId))
      .catch(noop);
  },
  pendingComponent: EventLoading,
  component: LatestEvent,
});

function LatestEvent() {
  const { id, issueId } = Route.useParams();
  const projectId = Number.parseInt(id, 10);
  // The issue too, so a missing one reads as a missing issue.
  useSuspenseQuery(issueQueries.detail(projectId, issueId));
  const { data: latest } = useSuspenseQuery(
    eventQueries.latest(projectId, issueId),
  );

  return latest ? (
    <Navigate
      replace
      to="/projects/$id/issues/$issueId/events/$eventId"
      params={{ id, issueId, eventId: String(latest.id) }}
    />
  ) : (
    <Navigate
      replace
      to="/projects/$id/issues/$issueId/events/empty"
      params={{ id, issueId }}
    />
  );
}
