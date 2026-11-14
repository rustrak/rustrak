import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { savedViewQueries } from '@/features/saved-view/api/queries';
import {
  defaultView,
  searchOf,
  viewQuery,
} from '@/features/saved-view/model/saved-view';

/**
 * Where a page entered with no filters goes: its default view (the reader's,
 * then the project's), for the route to `<Navigate replace />` to. Only until
 * the filters first change: clearing them later is a choice, not a reason to
 * put the default back.
 */
export function useEntryView(
  projectId: number | undefined,
  page: string,
  search: Record<string, unknown>,
): Record<string, string> | undefined {
  const bare = viewQuery(search) === '';
  const [moved, setMoved] = useState(false);
  if (!bare && !moved) setMoved(true);
  const views = useQuery(savedViewQueries.list(projectId, page)).data;
  const view = !moved && bare && views ? defaultView(views) : undefined;
  return view && searchOf(view.query);
}
