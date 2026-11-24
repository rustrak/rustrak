/**
 * Writes for the project feature.
 *
 * The rate limits a project update can set are its own, read back through its
 * detail; the server-wide `rateLimits` read never changes from here.
 */
import type {
  CreateProject,
  SidebarProjects,
  UpdateProject,
} from '@rustrak/client';
import { mutationOptions } from '@tanstack/react-query';
import { projectScope } from '@/shared/api/keys';
import { orThrow } from '@/shared/api/query';
import { api } from '@/shared/api/rustrak';
import { projectQueries } from './queries';

export const projectMutations = {
  create: () =>
    mutationOptions({
      mutationFn: (body: CreateProject) =>
        orThrow(api().projects.create({ body })),
      meta: { invalidates: [projectQueries.lists()] },
    }),
  /** Name and platform show in every list and the switcher, not only here. */
  update: (id: number) =>
    mutationOptions({
      mutationFn: (body: UpdateProject) =>
        orThrow(api().projects.update({ path: { id }, body })),
      meta: {
        invalidates: [
          projectQueries.detail(id).queryKey,
          projectQueries.lists(),
        ],
      },
    }),
  /**
   * The sidebar follows a drag or a switch at once: the cache takes the new
   * value before the request, and the old one back if the server refuses it.
   */
  saveSidebar: () =>
    mutationOptions({
      // Each save sends the whole setup, so they run one after another, in
      // the order they were made: two in flight could land out of order and
      // the older one would win.
      scope: { id: 'sidebar-projects' },
      mutationFn: (body: SidebarProjects) =>
        orThrow(api().sidebar.updateProjects({ body })),
      onMutate: async (body, { client }) => {
        const { queryKey } = projectQueries.sidebar();
        await client.cancelQueries({ queryKey });
        const before = client.getQueryData(queryKey);
        client.setQueryData(queryKey, body);
        return { before };
      },
      onError: (_error, _body, saved, { client }) => {
        if (saved?.before) {
          client.setQueryData(projectQueries.sidebar().queryKey, saved.before);
        }
      },
      meta: { invalidates: [projectQueries.sidebar().queryKey] },
    }),
  /**
   * The id is the mutation's variable: the projects grid deletes whichever
   * card was confirmed, and its pending `variables` is the card to hide.
   */
  delete: () =>
    mutationOptions({
      mutationFn: (id: number) =>
        orThrow(api().projects.delete({ path: { id } })),
      // Everything of the project goes before the lists refetch: asking
      // again for any of it can only answer 404.
      onSuccess: (_data, id, _result, { client }) =>
        client.removeQueries({ queryKey: projectScope(id) }),
      meta: { invalidates: [projectQueries.lists()] },
    }),
};
