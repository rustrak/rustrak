import type {
  Issue,
  IssueFilter,
  OffsetPaginatedResponse,
} from '@rustrak/client';
import { useNavigate } from '@tanstack/react-router';
import { AlertCircle, Check, Trash2, VolumeX, X } from 'lucide-react';
import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import { toast } from 'sonner';
import { useFormatter, useTranslations } from 'use-intl';
import {
  bulkDeleteIssues,
  bulkUpdateIssues,
  deleteIssue,
} from '@/features/issue/api/mutations';
import { invalidateIssues } from '@/features/issue/api/queries';
import { type IssueAction, STATUS_FOR } from '@/features/issue/model/actions';
import { DataTable } from '@/shared/ui/components/data-table/data-table';
import { DataTablePagination } from '@/shared/ui/components/data-table/pagination';
import { useAppTable } from '@/shared/ui/components/data-table/use-app-table';
import { Button } from '@/shared/ui/components/shadcn/button';
import { useTableUrlState } from '@/shared/ui/hooks/use-table-url-state';
import { DeleteIssuesDialog } from './delete-issues-dialog';
import { type IssueRowHandlers, issueColumns } from './issue-columns';
import { IssueFilters } from './issue-filters';

interface IssuesListProps {
  projectId: number;
  issues: OffsetPaginatedResponse<Issue>;
  currentFilter: IssueFilter;
  currentPage: number;
}

export function IssuesList({
  projectId,
  issues: page,
  currentFilter,
  currentPage,
}: IssuesListProps) {
  const t = useTranslations('issues');
  const tableT = useTranslations('table');
  const format = useFormatter();
  const navigate = useNavigate({ from: '/projects/$id/issues/' });
  const { items: issues, total_count, per_page } = page;

  // Null means "no single issue targeted", which is how the dialog tells a row
  // delete from a batch delete without a second flag to keep in step.
  const [pendingDelete, setPendingDelete] = useState<Issue | null>(null);
  const [deleteOpen, setDeleteOpen] = useState(false);

  const urlState = useTableUrlState({
    page: currentPage,
    perPage: per_page,
    navigate: ({ page }) => navigate({ search: (prev) => ({ ...prev, page }) }),
  });

  /**
   * What the action cells call, read at event time.
   *
   * The columns are built once per project rather than once per render, so
   * the cells cannot capture this render's callbacks; they reach through a ref
   * whose identity never changes, refreshed below once `table` exists. The
   * placeholders are never reachable: layout effects run before the user can
   * click anything.
   */
  const handlers = useRef<IssueRowHandlers>({
    onAction: () => {},
    onDelete: () => {},
  });

  const columns = useMemo(
    () => issueColumns(projectId, handlers, t, tableT, format),
    [projectId, t, tableT, format],
  );

  const table = useAppTable({
    data: issues,
    columns,
    getRowId: (issue) => issue.id,
    rowCount: total_count,
    enableRowSelection: true,
    state: { pagination: urlState.pagination },
    onPaginationChange: urlState.onPaginationChange,
  });

  const selectedIds = () =>
    table.getSelectedRowModel().rows.map((row) => row.original.id);

  const applyAction = (action: IssueAction, ids: readonly string[]) => {
    urlState.run(async () => {
      // The failure is a returned value now, so an unchecked call would clear
      // the selection and refresh back to the old state with nothing said.
      const result = await bulkUpdateIssues(projectId, {
        ids: [...ids],
        status: STATUS_FOR[action],
      });

      if (!result.success) {
        toast.error(t('toasts.updateFailed'), {
          description: result.error.message,
        });
        return;
      }

      table.resetRowSelection();
      await invalidateIssues(projectId);
    });
  };

  const confirmDelete = () => {
    urlState.run(async () => {
      const result = pendingDelete
        ? await deleteIssue(projectId, pendingDelete.id)
        : await bulkDeleteIssues(projectId, { ids: selectedIds() });

      if (!result.success) {
        toast.error(
          pendingDelete
            ? t('toasts.deleteOneFailed')
            : t('toasts.deleteManyFailed'),
          { description: result.error.message },
        );
        return;
      }

      if (!pendingDelete) table.resetRowSelection();
      setDeleteOpen(false);
      setPendingDelete(null);
      await invalidateIssues(projectId);
    });
  };

  useLayoutEffect(() => {
    handlers.current = {
      onAction: (action, issue) => applyAction(action, [issue.id]),
      onDelete: (issue) => {
        setPendingDelete(issue);
        setDeleteOpen(true);
      },
    };
  });

  const selectedCount = table.getSelectedRowModel().rows.length;

  return (
    <div className="flex h-full flex-col">
      <IssueFilters
        currentFilter={currentFilter}
        onFilterChange={(filter) =>
          navigate({ search: { filter, page: undefined } })
        }
        disabled={urlState.isPending}
      />

      <DataTable
        table={table}
        stickyHeader
        isPending={urlState.isPending}
        className="flex-1"
        bulkActions={
          <>
            <span className="text-xs font-semibold tabular-nums">
              {t('selectedCount', { count: selectedCount })}
            </span>
            <Button
              variant="outline"
              size="sm"
              className="h-7"
              onClick={() => applyAction('resolve', selectedIds())}
              disabled={urlState.isPending}
            >
              <Check className="size-3.5" />
              {t('actions.resolve')}
            </Button>
            <Button
              variant="outline"
              size="sm"
              className="h-7"
              onClick={() => applyAction('mute', selectedIds())}
              disabled={urlState.isPending}
            >
              <VolumeX className="size-3.5" />
              {t('actions.mute')}
            </Button>
            <Button
              variant="destructive"
              size="sm"
              className="h-7"
              onClick={() => {
                setPendingDelete(null);
                setDeleteOpen(true);
              }}
              disabled={urlState.isPending}
            >
              <Trash2 className="size-3.5" />
              {t('actions.delete')}
            </Button>
            <Button
              variant="ghost"
              size="icon"
              className="ml-auto size-7"
              aria-label={t('clearSelection')}
              onClick={() => table.resetRowSelection()}
            >
              <X className="size-3.5" />
            </Button>
          </>
        }
        empty={
          <div className="flex flex-col items-center justify-center px-6 py-16 text-center">
            <AlertCircle className="mb-4 size-12 text-muted-foreground/50" />
            <p className="text-muted-foreground">{t('empty.title')}</p>
            <p className="text-sm text-muted-foreground/70">
              {currentFilter === 'open'
                ? t('empty.allResolved')
                : t('empty.noFiltered', { filter: currentFilter })}
            </p>
          </div>
        }
      />

      <DataTablePagination table={table} disabled={urlState.isPending} />

      <DeleteIssuesDialog
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        count={pendingDelete ? 1 : selectedCount}
        isPending={urlState.isPending}
        onConfirm={confirmDelete}
      />
    </div>
  );
}
