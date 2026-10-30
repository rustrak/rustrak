import { noop, useQuery } from '@tanstack/react-query';
import { createFileRoute, Outlet } from '@tanstack/react-router';
import { projectQueries } from '@/features/project/api/queries';

/**
 * The project every screen under `/projects/$id` belongs to.
 *
 * Its loader starts the project's read, which the sidebar and the trail
 * (`-components/app-frame.tsx`) and every page below subscribe to.
 */
export const Route = createFileRoute('/_authenticated/projects/$id')({
  loader: ({ context: { queryClient }, params }) => {
    void queryClient
      .query(projectQueries.detail(Number.parseInt(params.id, 10)))
      .catch(noop);
  },
  component: ProjectLayout,
});

/**
 * A project that cannot be read fails here, once, for every page below:
 * including one that never reads it itself, like the new-monitor form, which
 * would otherwise offer to create a monitor on a project that is gone.
 *
 * Not by suspending, which would hold every page's own skeleton back behind
 * this read. The read runs beside the page and throws into this route's
 * boundary only when it fails with nothing to show; a background refetch
 * that fails leaves the page alone.
 */
function ProjectLayout() {
  const { id } = Route.useParams();
  useQuery({
    ...projectQueries.detail(Number.parseInt(id, 10)),
    throwOnError: (_error, query) => query.state.data === undefined,
  });
  return <Outlet />;
}
