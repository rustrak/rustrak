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
import { afterFailedSave } from '@/features/project/model/sidebar';
import { projectScope } from '@/shared/api/keys';
import { type ApiError, orThrow } from '@/shared/api/query';
import { api } from '@/shared/api/rustrak';
import { projectQueries, sidebarConfirmed } from './queries';

const SIDEBAR_SAVE = ['sidebar', 'projects', 'save'] as const;

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
   * `SidebarOptions`), which builds each change from the cache so two quick
   * clicks do not undo each other. The saves queue (`scope`), so they reach
   * the server in the order they were made. No `meta.invalidates`, on
   * purpose: a refetch after each save could land between two of them and
   * bring the older setup back. The cache is the truth until the last
   * queued save fails; then it goes back to the last confirmed setup and
   * reads it again.
   */
  saveSidebar: (onFailure: (error: ApiError) => void) =>
    mutationOptions({
      mutationKey: SIDEBAR_SAVE,
      scope: { id: 'sidebar-projects' },
      mutationFn: (body: SidebarProjects) =>
        orThrow(api().sidebar.updateProjects({ body })),
      onSuccess: (saved) => sidebarConfirmed.set(saved),
      // The last save in the queue settles the screen: back to the last
      // confirmed setup, so a server that is down does not leave unsaved
      // clicks on screen, then read again, since a PUT that failed on the way
      // back may still have been stored. With a save queued behind, nothing
      // yet: it carries this change, and a read now could land before it.
      onError: (error, _body, _result, { client }) => {
        const { queryKey } = projectQueries.sidebar();
        // This save is still counted while its onError runs.
        const laterSaveQueued =
          client.isMutating({ mutationKey: SIDEBAR_SAVE }) > 1;
        client.setQueryData(
          queryKey,
          afterFailedSave(
            client.getQueryData(queryKey),
            sidebarConfirmed.get(),
            laterSaveQueued,
          ),
        );
        if (!laterSaveQueued) void client.invalidateQueries({ queryKey });
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
