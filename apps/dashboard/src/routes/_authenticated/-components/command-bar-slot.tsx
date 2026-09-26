import { useQuery } from '@tanstack/react-query';
import { projectQueries } from '@/features/project/api/queries';
import { toCommandProjects } from '@/features/project/lib/command-items';
import { CommandBar } from '@/shared/ui/components/command-bar/command-bar';

/**
 * Composition seam for the command bar: it spans the `project` slice and the
 * static settings routes, so neither feature can own it and it is assembled
 * here instead.
 *
 * The header must paint before the projects read lands, so it does not
 * suspend. It is the same cache entry the project sidebar reads. A
 * failed or pending read still renders the bar — the static commands are the
 * bulk of it, and a search box that silently disappears is worse than one
 * missing project entries.
 *
 * One page, on purpose: see `COMMAND_BAR_PROJECT_LIMIT` for why the bar has a
 * stated ceiling instead of paging until the instance runs out.
 */
export function CommandBarSlot() {
  const { data } = useQuery(projectQueries.all());
  const projects = data?.success ? data.data.items : [];

  return <CommandBar projects={toCommandProjects(projects)} />;
}
