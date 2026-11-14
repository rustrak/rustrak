/**
 * Writes to saved views, a project's or the workspace's (`projectId`
 * undefined). Each refreshes every page's views of that scope: there are
 * few, and a default moved on one page is read there only.
 */
import type {
  CreateSavedView,
  SetDefaultView,
  UpdateSavedView,
} from '@rustrak/client';
import { mutationOptions } from '@tanstack/react-query';
import { orThrow } from '@/shared/api/query';
import { api } from '@/shared/api/rustrak';
import { savedViewQueries } from './queries';

export const savedViewMutations = {
  create: (projectId?: number) =>
    mutationOptions({
      mutationFn: (body: CreateSavedView) =>
        orThrow(
          projectId === undefined
            ? api().workspaceViews.create({ body })
            : api().savedViews.create({
                path: { project_id: projectId },
                body,
              }),
        ),
      meta: { invalidates: [savedViewQueries.all(projectId)] },
    }),
  update: (projectId?: number) =>
    mutationOptions({
      mutationFn: ({ id, ...body }: UpdateSavedView & { id: number }) =>
        orThrow(
          projectId === undefined
            ? api().workspaceViews.update({ path: { view_id: id }, body })
            : api().savedViews.update({
                path: { project_id: projectId, view_id: id },
                body,
              }),
        ),
      meta: { invalidates: [savedViewQueries.all(projectId)] },
    }),
  remove: (projectId?: number) =>
    mutationOptions({
      mutationFn: (id: number) =>
        orThrow(
          projectId === undefined
            ? api().workspaceViews.delete({ path: { view_id: id } })
            : api().savedViews.delete({
                path: { project_id: projectId, view_id: id },
              }),
        ),
      meta: { invalidates: [savedViewQueries.all(projectId)] },
    }),
  setDefault: (projectId?: number) =>
    mutationOptions({
      mutationFn: (body: SetDefaultView) =>
        orThrow(
          projectId === undefined
            ? api().workspaceViews.setDefault({ body })
            : api().savedViews.setDefault({
                path: { project_id: projectId },
                body,
              }),
        ),
      meta: { invalidates: [savedViewQueries.all(projectId)] },
    }),
};
