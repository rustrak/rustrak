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
import { type ApiError, orThrow } from '@/shared/api/query';
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
  /**
   * Saves the setup the caller has already put in the cache (see
   * `SidebarOptions`). Not the usual `onMutate` and `meta.invalidates`, on
   * purpose: the saves queue (`scope`), so each one runs in the order it was
   * made, and a queued save's `onMutate` would wait its turn too, leaving the
   * click unseen; and a refetch after each save could land between two of
   * them and bring the older setup back. The cache is the truth until a save
   * fails, and then the server's setup is read again.
   */
  saveSidebar: (onFailure: (error: ApiError) => void) =>
    mutationOptions({
      scope: { id: 'sidebar-projects' },
      mutationFn: (body: SidebarProjects) =>
        orThrow(api().sidebar.updateProjects({ body })),
      onError: (error, _body, _result, { client }) => {
        void client.invalidateQueries({
          queryKey: projectQueries.sidebar().queryKey,
        });
        onFailure(error);
      },
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
