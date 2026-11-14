import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { savedViewQueries } from '@/features/saved-view/api/queries';
import {
  bareSearch,
  defaultView,
  searchOf,
  viewQuery,
} from '@/features/saved-view/model/saved-view';

/**
 * Where a page entered with nothing in its address goes: its default view
 * (the reader's, then the project's), read through the route's `validate`,
 * for the route to `<Navigate replace />` to.
 *
 * `search` is the whole address: an asked-for page or an open line is a
 * destination of its own, not a bare visit. Only until the address first
 * changes on this project and page, since clearing the filters later is a
 * choice; and never to a default that validates to nothing, which would
 * send the page to itself forever.
 */
export function useEntryView<T extends object>(
  projectId: number | undefined,
  page: string,
  search: Record<string, unknown>,
  validate: (search: Record<string, unknown>) => T,
): T | undefined {
  const bare = bareSearch(search);
  const scope = `${projectId ?? 'workspace'}:${page}`;
  const [moved, setMoved] = useState<string>();
  if (!bare && moved !== scope) setMoved(scope);
  const views = useQuery(savedViewQueries.list(projectId, page)).data;
  const view =
    moved !== scope && bare && views ? defaultView(views) : undefined;
  if (!view) return undefined;
  const target = validate(searchOf(view.query));
  return viewQuery(target as Record<string, unknown>) === ''
    ? undefined
    : target;
}
