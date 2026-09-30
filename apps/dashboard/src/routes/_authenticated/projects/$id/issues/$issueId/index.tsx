import { createFileRoute, redirect } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { getLastEvent } from '@/features/event/api/queries';
import { getIssue } from '@/features/issue/api/queries';
import { searchString } from '@/shared/lib/search-params';
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
  validateSearch: (search: Record<string, unknown>) => ({
    environment: searchString(search.environment),
  }),
  beforeLoad: async ({ params, search }) => {
    const projectId = Number.parseInt(params.id, 10);

    // Verify issue exists
    const issue = await getIssue(projectId, params.issueId, search.environment);
    if (!issue.success) {
      return {
        failure: { error: issue.error, title: 'loadIssueFailed' as const },
      };
    }

    const lastEvent = await getLastEvent(
      projectId,
      params.issueId,
      search.environment,
    );

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
        href: `/projects/${projectId}/issues/${params.issueId}/events/${lastEvent.data.id}${search.environment ? `?environment=${encodeURIComponent(search.environment)}` : ''}`,
      });
    }

    // If no events, show empty state
    throw redirect({
      href: `/projects/${projectId}/issues/${params.issueId}/events/empty${search.environment ? `?environment=${encodeURIComponent(search.environment)}` : ''}`,
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
