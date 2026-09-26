import type { OffsetPaginatedResponse, Project } from '@rustrak/client';
import { Link, useNavigate } from '@tanstack/react-router';
import { FolderOpen, Loader2, Plus, Trash2 } from 'lucide-react';
import { useOptimistic, useState, useTransition } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { deleteProject } from '@/features/project/api/mutations';
import { PROJECT_COLUMNS } from '@/features/project/model/columns';
import { invalidate, scope } from '@/shared/api/query-client';
import { cn } from '@/shared/lib/utils';
import { Button } from '@/shared/ui/components/shadcn/button';
import { Checkbox } from '@/shared/ui/components/shadcn/checkbox';
import { TablePagination } from '@/shared/ui/components/table-pagination';
import { useRowSelection } from '@/shared/ui/hooks/use-row-selection';
import { DeleteProjectsDialog } from './delete-projects-dialog';
import { ProjectRow } from './project-row';

/** Shared styling for every column header in the table. */
const HEADER =
  'text-xs font-bold uppercase tracking-widest text-muted-foreground';

interface ProjectsListProps {
  initialProjects: OffsetPaginatedResponse<Project>;
  currentPage: number;
}

export function ProjectsList({
  initialProjects,
  currentPage,
}: ProjectsListProps) {
  const t = useTranslations('projects');
  const navigate = useNavigate({ from: '/projects/' });
  const [isPending, startTransition] = useTransition();
  const {
    items: serverProjects,
    total_count,
    total_pages,
    per_page,
  } = initialProjects;

  // Optimistic state for immediate UI feedback on deletion
  const [projects, removeOptimistic] = useOptimistic(
    serverProjects,
    (state, deletedIds: number[]) => {
      // A Set because this is a filter over every row: with the array, a batch
      // delete of n rows out of m scans n*m times.
      const gone = new Set(deletedIds);
      return state.filter((p) => !gone.has(p.id));
    },
  );

  const selection = useRowSelection(projects.map((p) => p.id));

  // Null means "no single project targeted", which is how the dialog tells a
  // row delete from a batch delete without a second flag to keep in step.
  const [pendingDelete, setPendingDelete] = useState<Project | null>(null);
  const [deleteOpen, setDeleteOpen] = useState(false);

  const confirmDelete = () => {
    const idsToDelete = pendingDelete ? [pendingDelete.id] : [...selection.ids];

    if (idsToDelete.length === 0) return;

    // Close the dialog immediately: the rows are about to vanish optimistically
    // and a confirmation still sitting over them reads as a failed click.
    setDeleteOpen(false);

    startTransition(async () => {
      removeOptimistic(idsToDelete);

      for (const id of idsToDelete) {
        const result = await deleteProject(id);

        if (!result.success) {
          // Stop at the first failure rather than carrying on: the optimistic
          // removal is reverted by the refresh below, and continuing would
          // report "3 projects deleted" over a set where one survived.
          toast.error(t('toasts.deleteFailed'), {
            description: result.error.message,
          });
          void invalidate(scope.projects);
          return;
        }
      }

      toast.success(t('toasts.deleted', { count: idsToDelete.length }));

      if (!pendingDelete) selection.clear();
      setPendingDelete(null);
      void invalidate(scope.projects);
    });
  };

  return (
    <div className="flex flex-col h-full">
      {selection.count > 0 && (
        <div className="shrink-0 flex items-center justify-end gap-2 mb-4">
          <span className="text-sm text-muted-foreground">
            {t('selectedCount', { count: selection.count })}
          </span>
          <Button
            variant="destructive"
            size="sm"
            onClick={() => {
              setPendingDelete(null);
              setDeleteOpen(true);
            }}
            disabled={isPending}
          >
            <Trash2 className="mr-1 size-3" />
            {t('delete')}
          </Button>
        </div>
      )}

      {projects.length === 0 ? (
        <div className="flex-1 flex flex-col items-center justify-center text-center">
          <FolderOpen className="size-12 text-muted-foreground/50 mb-4" />
          <p className="text-muted-foreground">{t('empty.title')}</p>
          <p className="text-sm text-muted-foreground/70">{t('empty.hint')}</p>
          <Button
            className="mt-4"
            nativeButton={false}
            render={<Link to="/projects/new" />}
          >
            <Plus className="mr-2 size-4" />
            {t('newProject')}
          </Button>
        </div>
      ) : (
        <div className="flex-1 overflow-hidden flex flex-col border rounded-lg">
          <div className="shrink-0 flex items-center gap-4 px-4 py-3 bg-muted/50 border-b">
            <Checkbox
              checked={selection.allSelected}
              onCheckedChange={selection.toggleAll}
            />
            {/* Stands in for the platform icon on each row. Without it the
                header's flex tracks are computed over a row 28px wider than
                the real ones and every column below drifts left. */}
            <span className="w-7 shrink-0" aria-hidden />
            <span className={cn(HEADER, PROJECT_COLUMNS.name)}>
              {t('columns.project')}
            </span>
            <span className={cn(HEADER, PROJECT_COLUMNS.issues)}>
              {t('columns.issues')}
            </span>
            <span className={cn(HEADER, PROJECT_COLUMNS.events)}>
              {t('columns.events24h')}
            </span>
            <span className={cn(HEADER, PROJECT_COLUMNS.total)}>
              {t('columns.total')}
            </span>
            <span className={cn(HEADER, PROJECT_COLUMNS.trend)}>
              {t('columns.issueActivity')}
            </span>
            {/* Demoted from `sm` to `xl`: on a narrow viewport the age of a
                project loses to every column above, all of which say
                something about right now. */}
            <span className={cn(HEADER, PROJECT_COLUMNS.created)}>
              {t('columns.created')}
            </span>
            <span className="w-8" />
          </div>

          <div className="flex-1 overflow-auto">
            {projects.map((project) => (
              <ProjectRow
                key={project.id}
                project={project}
                selected={selection.isSelected(project.id)}
                onToggleSelect={() => selection.toggle(project.id)}
                onDelete={(target) => {
                  setPendingDelete(target);
                  setDeleteOpen(true);
                }}
              />
            ))}
          </div>
        </div>
      )}

      <TablePagination
        currentPage={currentPage}
        totalPages={total_pages}
        totalCount={total_count}
        perPage={per_page}
        disabled={isPending}
        onPageChange={(page) =>
          navigate({ search: (prev) => ({ ...prev, page }) })
        }
      />

      {isPending && (
        <div className="absolute inset-0 bg-background/50 flex items-center justify-center">
          <Loader2 className="size-8 animate-spin text-primary" />
        </div>
      )}

      <DeleteProjectsDialog
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        count={selection.count}
        targetName={pendingDelete?.name}
        isPending={isPending}
        onConfirm={confirmDelete}
      />
    </div>
  );
}
