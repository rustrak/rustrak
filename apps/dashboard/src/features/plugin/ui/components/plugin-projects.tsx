import type { PluginSummary, ProjectResponse } from '@rustrak/client';
import { setupHref } from '@rustrak/plugin/ui';
import {
  ChevronRightIcon,
  createDataTableColumnHelper,
  DataTable,
  type DataTableQuery,
  SettingRow,
  SettingSection,
  Switch,
  Tag,
  useDataTable,
} from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { useMemo, useState } from 'react';
import { useTranslations } from 'use-intl';
import { ProjectLogo } from '@/shared/ui/components/projects-picker';
import { useRouter } from '@/shared/ui/hooks/use-router';
import { useToastFailure } from '@/shared/ui/hooks/use-toast-failure';
import { pluginMutations } from '../../api/mutations';
import { activeIn, withProject } from '../../model/plugins';

const column = createDataTableColumnHelper<ProjectResponse>();
const PAGE_SIZE = 10;

/**
 * Where a plugin runs: everywhere, or the projects switched on in a table
 * that stays one screen however many projects there are. A row opens that
 * project's own setup of the plugin.
 */
export function PluginProjects({
  plugin,
  projects,
  canManage,
}: {
  plugin: PluginSummary;
  projects: ProjectResponse[];
  canManage: boolean;
}) {
  const t = useTranslations('settings');
  const toastFailure = useToastFailure();
  const router = useRouter();
  const configure = useMutation(pluginMutations.configure(plugin.id));
  const save = (body: { allProjects: boolean; projects?: number[] }) =>
    configure.mutate(body, {
      onError: (error) => toastFailure(t('plugins.configureFailed'), error),
    });

  const [query, setQuery] = useState<DataTableQuery>({
    sorting: [],
    filters: [],
    search: '',
    pagination: { pageIndex: 0, pageSize: PAGE_SIZE },
  });
  const start = query.pagination.pageIndex * PAGE_SIZE;
  const page = projects.slice(start, start + PAGE_SIZE);

  const columns = useMemo(() => {
    const toggle = (id: number, on: boolean) =>
      save({ allProjects: false, projects: withProject(plugin, id, on) });
    return column.columns([
      column.accessor('name', {
        id: 'project',
        header: t('plugins.detail.project'),
        enableSorting: false,
        enableHiding: false,
        // The row opens the project's setup: the second line says so.
        cell: ({ row }) => (
          <span className="flex min-w-0 items-center gap-2.5 py-1">
            <ProjectLogo project={row.original} />
            <span className="flex min-w-0 flex-col gap-0.5">
              <span className="truncate font-medium text-fg">
                {row.original.name}
              </span>
              <span className="truncate text-meta text-fg-subtle">
                {t('plugins.detail.openSetup')}
              </span>
            </span>
          </span>
        ),
        meta: { label: t('plugins.detail.project') },
      }),
      column.display({
        id: 'status',
        header: t('plugins.detail.status'),
        cell: ({ row }) =>
          activeIn(plugin, row.original.id) ? (
            <Tag tone="brand" variant="soft">
              {t('plugins.detail.runs')}
            </Tag>
          ) : (
            <span className="text-fg-subtle">{t('plugins.detail.off')}</span>
          ),
        meta: { label: t('plugins.detail.status'), width: 140 },
      }),
      column.display({
        id: 'active',
        header: () => (
          <span className="sr-only">{t('plugins.detail.active')}</span>
        ),
        cell: ({ row }) => (
          // A click on the switch is its own, not the row's.
          <span
            className="flex justify-end"
            onClick={(e) => e.stopPropagation()}
          >
            <Switch
              checked={activeIn(plugin, row.original.id)}
              disabled={!canManage || configure.isPending || plugin.allProjects}
              aria-label={t('plugins.detail.runIn', {
                project: row.original.name,
              })}
              onCheckedChange={(on) => toggle(row.original.id, on)}
            />
          </span>
        ),
        meta: { label: t('plugins.detail.active'), width: 80, align: 'end' },
      }),
      column.display({
        id: 'open',
        header: () => (
          <span className="sr-only">{t('plugins.detail.openSetup')}</span>
        ),
        cell: () => (
          <span className="flex justify-end text-fg-subtle">
            <ChevronRightIcon size="sm" />
          </span>
        ),
        meta: { label: t('plugins.detail.openSetup'), width: 40, align: 'end' },
      }),
    ]);
    // `save` is a fresh closure each render over the same mutation; React
    // Doctor reads this rule's directive.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [t, plugin, canManage, configure.isPending]);

  const table = useDataTable({
    data: page,
    columns,
    rowCount: projects.length,
    query,
    onQueryChange: (update) => setQuery(update),
    getRowId: (p) => String(p.id),
  });

  return (
    <SettingSection
      title={t('plugins.detail.projects')}
      description={t('plugins.detail.projectsHint')}
    >
      <SettingRow
        title={t('plugins.detail.allProjects')}
        description={t('plugins.detail.allProjectsHint')}
      >
        <div className="flex justify-end">
          <Switch
            checked={plugin.allProjects}
            disabled={!canManage || configure.isPending}
            aria-label={t('plugins.detail.allProjects')}
            onCheckedChange={(on) =>
              save(
                on
                  ? { allProjects: true }
                  : { allProjects: false, projects: [] },
              )
            }
          />
        </div>
      </SettingRow>
      {/* Its own stacking context: the table's sticky header stays under
          the page's when the settings scroll. */}
      <div
        className="isolate flex flex-col gap-3 px-5 py-4"
        data-testid="plugin-projects"
      >
        <DataTable
          table={table}
          onRowClick={(row) =>
            router.push(
              setupHref({ pluginId: plugin.id, projectId: row.original.id }),
            )
          }
        />
      </div>
    </SettingSection>
  );
}
