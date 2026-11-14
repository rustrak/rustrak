import type { SavedView } from '@rustrak/client';
import {
  Button,
  Checkbox,
  ChevronDownIcon,
  confirm,
  DeleteIcon,
  Dialog,
  DialogBody,
  DialogClose,
  DialogFooter,
  DialogHeader,
  Menu,
  type MenuAction,
  SaveViewIcon,
  StarIcon,
  UndoIcon,
} from '@rustrak/ui';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslations } from 'use-intl';
import { z } from 'zod';
import { savedViewMutations } from '@/features/saved-view/api/mutations';
import { savedViewQueries } from '@/features/saved-view/api/queries';
import {
  activeView,
  searchOf,
  type ViewMenuAction,
  viewMenu,
  viewQuery,
} from '@/features/saved-view/model/saved-view';
import type { ApiError } from '@/shared/api/query';
import { applyServerFieldErrors } from '@/shared/lib/form-errors';
import { revalidateLogic, useAppForm } from '@/shared/ui/components/app-form';
import { usePermissionsLater } from '@/shared/ui/hooks/use-permissions';
import { useSessionUser } from '@/shared/ui/hooks/use-session-user';
import { useToastFailure } from '@/shared/ui/hooks/use-toast-failure';

export interface SavedViewsProps {
  /** Undefined on a page that spans every project: the workspace's views. */
  projectId?: number;
  /** The page the views belong to: `issues`, `logs`… */
  page: string;
  /** The route's search, as it is now. */
  search: Record<string, unknown>;
  /** Goes to `search`; the route's `validateSearch` reads it. */
  onApply: (search: Record<string, string>) => void;
}

/**
 * The views of one page (#531): a menu that opens a saved address, saves the
 * one on screen, and marks the page's default. Any page whose state is its
 * URL gets them by rendering this next to its filters.
 *
 * Which view is open is read from the address, so a shared link shows its
 * name too. Changing a filter after opening one keeps it as the view being
 * edited: the trigger says so with a dot, and the menu offers to save the
 * change, save it as a new view, or go back. Opening the page on its default
 * is the route's (`useEntryView`).
 */
export function SavedViews({
  projectId,
  page,
  search,
  onApply,
}: SavedViewsProps) {
  const t = useTranslations('savedViews');
  const me = useSessionUser();
  const permissions = usePermissionsLater();
  const toastFailure = useToastFailure();
  const views = useQuery(savedViewQueries.list(projectId, page)).data;
  const update = useMutation(savedViewMutations.update(projectId));
  const remove = useMutation(savedViewMutations.remove(projectId));
  const setDefault = useMutation(savedViewMutations.setDefault(projectId));
  const [editing, setEditing] = useState<number>();
  const [saving, setSaving] = useState(false);

  const query = viewQuery(search);
  // The view last on screen stays the one being edited while its filters
  // change: React's way to keep what an earlier render showed.
  const active = activeView(views ?? [], search);
  if (active && active.id !== editing) setEditing(active.id);
  const menu = viewMenu({
    views: views ?? [],
    search,
    editing,
    me: me.id,
    isAdmin:
      projectId === undefined
        ? permissions.isOwner
        : permissions.can('project', 'update', projectId),
  });
  const current = menu.current;

  const open = (view: SavedView) => onApply(searchOf(view.query));
  const failed = (title: string) => ({
    onError: (error: ApiError) => toastFailure(title, error),
  });

  const run: Record<ViewMenuAction, (view: SavedView) => void> = {
    save: (view) =>
      update.mutate({ id: view.id, query }, failed(t('saveFailed'))),
    saveAs: () => setSaving(true),
    reset: open,
    myDefault: (view) =>
      setDefault.mutate(
        { page, scope: 'user', view_id: view.is_my_default ? null : view.id },
        failed(t('defaultFailed')),
      ),
    projectDefault: (view) =>
      setDefault.mutate(
        {
          page,
          scope: 'project',
          view_id: view.is_project_default ? null : view.id,
        },
        failed(t('defaultFailed')),
      ),
    delete: async (view) => {
      const sure = await confirm({
        title: t('deleteTitle', { name: view.name }),
        description: t('deleteDescription'),
        tone: 'danger',
        confirmLabel: t('delete'),
      });
      if (sure) remove.mutate(view.id, failed(t('deleteFailed')));
    },
  };

  const actions: MenuAction[] = [
    ...(views ?? []).map((view) => ({
      id: `view-${view.id}`,
      label: view.name,
      icon:
        view.is_my_default || view.is_project_default ? StarIcon : undefined,
      selected: view.id === current?.id,
      hint:
        view.visibility === 'owner'
          ? t('personal')
          : projectId === undefined
            ? t('sharedWorkspace')
            : t('shared'),
      onSelect: () => open(view),
    })),
    ...menu.actions.map(({ kind, view }, index) => ({
      ...LOOKS[kind],
      id: kind,
      label: label(t, kind, view, projectId === undefined),
      separated: index === 0 && Boolean(views?.length),
      onSelect: () => run[kind](view as SavedView),
    })),
  ];

  return (
    <>
      <Menu
        align="end"
        trigger={
          <Button
            variant="secondary"
            icon={SaveViewIcon}
            aria-label={t('trigger')}
          >
            <span className="max-w-40 truncate">
              {current?.name ?? t('trigger')}
            </span>
            {menu.edited ? (
              <span
                role="img"
                aria-label={t('unsaved')}
                className="size-1.5 shrink-0 rounded-full bg-fg-brand"
              />
            ) : null}
            <ChevronDownIcon size="sm" aria-hidden="true" />
          </Button>
        }
        actions={
          actions.length
            ? actions
            : [{ id: 'empty', label: t('empty'), disabled: true }]
        }
      />
      <SaveViewDialog
        open={saving}
        onOpenChange={setSaving}
        projectId={projectId}
        page={page}
        query={query}
      />
    </>
  );
}

const LOOKS: Record<ViewMenuAction, Pick<MenuAction, 'icon' | 'tone'>> = {
  save: { icon: SaveViewIcon },
  saveAs: { icon: SaveViewIcon },
  reset: { icon: UndoIcon },
  myDefault: { icon: StarIcon },
  projectDefault: { icon: StarIcon },
  delete: { icon: DeleteIcon, tone: 'danger' },
};

function label(
  t: ReturnType<typeof useTranslations<'savedViews'>>,
  kind: ViewMenuAction,
  view: SavedView | undefined,
  workspace: boolean,
): string {
  switch (kind) {
    case 'save':
      return t('saveChanges', { name: view?.name ?? '' });
    case 'saveAs':
      return t('saveAsNew');
    case 'myDefault':
      return view?.is_my_default ? t('unsetMyDefault') : t('setMyDefault');
    case 'projectDefault':
      if (workspace) {
        return view?.is_project_default
          ? t('unsetWorkspaceDefault')
          : t('setWorkspaceDefault');
      }
      return view?.is_project_default
        ? t('unsetProjectDefault')
        : t('setProjectDefault');
    case 'reset':
      return t('reset');
    case 'delete':
      return t('delete');
  }
}

function SaveViewDialog({
  open,
  onOpenChange,
  projectId,
  page,
  query,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  projectId: number | undefined;
  page: string;
  query: string;
}) {
  const t = useTranslations('savedViews');
  const globalT = useTranslations();
  const create = useMutation(savedViewMutations.create(projectId));
  const share = projectId === undefined ? t('shareWorkspace') : t('share');

  const form = useAppForm({
    defaultValues: { name: '', shared: false },
    validationLogic: revalidateLogic(),
    validators: {
      onDynamic: z.object({
        name: z.string().trim().min(1, t('nameRequired')).max(100),
        shared: z.boolean(),
      }),
    },
    onSubmit: ({ value }) =>
      create.mutate(
        {
          page,
          query,
          name: value.name,
          visibility: value.shared ? 'project' : 'owner',
        },
        {
          onSuccess: close,
          onError: (error) =>
            applyServerFieldErrors(form, error.error, {
              labels: { name: t('name') },
              t: globalT,
            }),
        },
      ),
  });

  const close = () => {
    form.reset();
    onOpenChange(false);
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => (next ? onOpenChange(true) : close())}
      size="sm"
    >
      <DialogHeader title={t('saveAsNew')} description={t('dialogHint')} />
      <form.AppForm>
        <form.Form className="flex min-h-0 flex-col">
          <DialogBody className="flex flex-col gap-4">
            <form.AppField name="name">
              {(field) => (
                <field.TextField
                  label={t('name')}
                  placeholder={t('namePlaceholder')}
                  autoFocus
                  disabled={create.isPending}
                />
              )}
            </form.AppField>
            <form.AppField name="shared">
              {(field) => (
                <label className="flex items-center gap-2 text-sm">
                  <Checkbox
                    aria-label={share}
                    checked={field.state.value}
                    onCheckedChange={(checked) => field.handleChange(checked)}
                    disabled={create.isPending}
                  />
                  {share}
                </label>
              )}
            </form.AppField>
            <form.RootError />
          </DialogBody>
          <DialogFooter>
            <DialogClose
              disabled={create.isPending}
              render={<Button variant="secondary">{t('cancel')}</Button>}
            />
            <Button type="submit" variant="primary" loading={create.isPending}>
              {t('save')}
            </Button>
          </DialogFooter>
        </form.Form>
      </form.AppForm>
    </Dialog>
  );
}
