import type { ProjectResponse, ProjectsListResponse } from '@rustrak/client';
import { Button, confirm, EmptyIcon, NewIcon, Text } from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { projectMutations } from '@/features/project/api/mutations';
import { Link } from '@/shared/ui/components/link';
import { usePermissionsLater } from '@/shared/ui/hooks/use-permissions';
import { useRouter } from '@/shared/ui/hooks/use-router';
import { useToastFailure } from '@/shared/ui/hooks/use-toast-failure';
import { ProjectCard } from './project-card';
import { TablePagination } from './table-pagination';

interface ProjectsGridProps {
  page: ProjectsListResponse;
  currentPage: number;
  /** The next page is on its way: the pager waits for it. */
  loading?: boolean;
}

/** Every project the reader can see, one card each, a page at a time. */
export function ProjectsGrid({
  page,
  currentPage,
  loading = false,
}: ProjectsGridProps) {
  const t = useTranslations('projects');
  const toastFailure = useToastFailure();
  const permissions = usePermissionsLater();
  const canCreate = permissions.can('projects', 'create');
  const router = useRouter();
  const remove = useMutation(projectMutations.delete());
  const { items, total_count, total_pages, per_page } = page;

  // The card leaves at once and stays gone until the list has reloaded
  // (`mutateAsync` settles after the refetch); a failed delete puts it back.
  const projects = remove.isPending
    ? items.filter((p) => p.id !== remove.variables)
    : items;
  const isPending = remove.isPending;
  const confirmDelete = async (target: ProjectResponse) => {
    const ok = await confirm({
      title: t('deleteDialog.titleNamed', { name: target.name }),
      description: t('deleteDialog.description', {
        subject: t('deleteDialog.subject', { count: 1 }),
      }),
      tone: 'danger',
      confirmLabel: t('deleteDialog.confirm'),
      cancelLabel: t('deleteDialog.cancel'),
    });
    if (!ok) return;
    remove.mutate(target.id, {
      onSuccess: () => toast.success(t('toasts.deleted', { count: 1 })),
      onError: (error) => toastFailure(t('toasts.deleteFailed'), error),
    });
  };

  if (projects.length === 0) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-1.5 py-24 text-center">
        <EmptyIcon size="2xl" className="mb-2 text-fg-ghost" />
        <Text variant="card-title">{t('empty.title')}</Text>
        <Text variant="meta" tone="subtle">
          {canCreate ? t('empty.hint') : t('empty.noAccessHint')}
        </Text>
        {canCreate ? (
          <Button
            variant="primary"
            icon={NewIcon}
            className="mt-4"
            render={<Link href="/projects/new" />}
          >
            {t('newProject')}
          </Button>
        ) : null}
      </div>
    );
  }

  return (
    <>
      <div className="grid grid-cols-1 gap-3.5 lg:grid-cols-2 2xl:grid-cols-3">
        {projects.map((project) => (
          <ProjectCard
            key={project.id}
            project={project}
            onDelete={
              permissions.can('project', 'delete', project.id)
                ? confirmDelete
                : undefined
            }
          />
        ))}
      </div>

      {total_pages > 1 ? (
        <TablePagination
          currentPage={currentPage}
          totalPages={total_pages}
          totalCount={total_count}
          perPage={per_page}
          disabled={isPending || loading}
          onPageChange={(page) => router.push(`/projects?page=${page}`)}
        />
      ) : null}
    </>
  );
}
