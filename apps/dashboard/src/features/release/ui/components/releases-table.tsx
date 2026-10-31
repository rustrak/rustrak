import type { ReleaseSummary, ReleasesListResponse } from '@rustrak/client';
import {
  createDataTableColumnHelper,
  DataTable,
  DataTableColumnsButton,
  type DataTableQuery,
  EnvironmentIcon,
  QueryBar,
  type QueryField,
  useDataTable,
} from '@rustrak/ui';
import { type ReactNode, useMemo } from 'react';
import { useTranslations } from 'use-intl';
import { RelativeTime } from '@/shared/ui/components/relative-time';
import { useColumnFormatter } from '@/shared/ui/hooks/use-column-formatter';
import { useRouter } from '@/shared/ui/hooks/use-router';
import { AdoptionBar, HealthRate, VersionName } from './release-figures';

const column = createDataTableColumnHelper<ReleaseSummary>();

/** One empty list, so the table's data is stable while the first page loads. */
const NO_RELEASES: ReleaseSummary[] = [];

/** What the list is scoped to, carried into a release's own page. */
interface ReleaseScope {
  environment?: string;
  period?: string;
}

/**
 * A release's own page; the version is a path segment. It opens on the list's
 * scope, so its figures are the ones its row showed.
 */
function releaseHref(projectId: number, version: string, scope: ReleaseScope) {
  const search = new URLSearchParams();
  if (scope.environment) search.set('environment', scope.environment);
  if (scope.period) search.set('period', scope.period);
  const query = search.size > 0 ? `?${search}` : '';
  return `/projects/${projectId}/releases/${encodeURIComponent(version)}${query}`;
}

/**
 * The release list. The first column says which release -- the version,
 * then in one quiet line how old it is and where it ran -- and every other
 * column is a figure read across releases, in one weight, coloured only
 * where a rate needs looking at.
 */
export function ReleasesTable({
  projectId,
  page,
  query,
  onQueryChange,
  loading,
  environments,
  period,
  scope,
}: {
  projectId: number;
  /** Absent while the first page loads: the table draws skeleton rows. */
  page?: ReleasesListResponse;
  query: DataTableQuery;
  onQueryChange: (
    updater: (previous: DataTableQuery) => DataTableQuery,
  ) => void;
  loading: boolean;
  /** For the query bar's `environment:` suggestions. */
  environments: string[];
  /** The window, beside the query bar. */
  period: ReactNode;
  scope: ReleaseScope;
}) {
  const t = useTranslations('releases');
  const format = useColumnFormatter();
  const router = useRouter();

  const columns = useMemo(() => {
    const count = (value: number) => (
      <span
        className={
          value === 0 ? 'text-fg-ghost' : 'text-fg-secondary tabular-nums'
        }
      >
        {format.number(value, 'compact')}
      </span>
    );
    const figure = { align: 'end' as const };

    return column.columns([
      column.accessor('version', {
        id: 'release',
        header: t('columns.release'),
        enableHiding: false,
        enableSorting: false,
        cell: ({ row }) => {
          const release = row.original;
          return (
            <div className="flex min-w-0 flex-col gap-0.5 py-1">
              <VersionName
                version={release.version}
                className="font-medium text-value"
              />
              <span className="truncate text-fg-subtle text-meta">
                <span
                  title={format.dateTime(
                    new Date(release.date_created),
                    'dateTime',
                  )}
                >
                  <RelativeTime at={release.date_created} />
                </span>
                {release.environments.length > 0
                  ? ` · ${release.environments.join(', ')}`
                  : null}
              </span>
            </div>
          );
        },
        meta: { label: t('columns.release') },
      }),
      column.accessor('adoption', {
        id: 'adoption',
        header: t('columns.adoption'),
        enableSorting: false,
        cell: ({ getValue }) => <AdoptionBar adoption={getValue()} />,
        meta: { label: t('columns.adoption'), width: 140 },
      }),
      column.accessor('crash_free_sessions', {
        id: 'crash_free_sessions',
        header: t('columns.crashFreeSessions'),
        enableSorting: false,
        cell: ({ getValue }) => <HealthRate rate={getValue()} quiet />,
        meta: { label: t('columns.crashFreeSessions'), width: 116, ...figure },
      }),
      column.accessor('crash_free_users', {
        id: 'crash_free_users',
        header: t('columns.crashFreeUsers'),
        enableSorting: false,
        cell: ({ getValue }) => <HealthRate rate={getValue()} quiet />,
        meta: { label: t('columns.crashFreeUsers'), width: 168, ...figure },
      }),
      column.accessor('sessions', {
        id: 'sessions',
        header: t('columns.sessions'),
        enableSorting: false,
        cell: ({ getValue }) => count(getValue()),
        meta: { label: t('columns.sessions'), width: 104, ...figure },
      }),
      column.accessor('new_issues', {
        id: 'new_issues',
        header: t('columns.newIssues'),
        enableSorting: false,
        cell: ({ getValue }) => count(getValue()),
        meta: { label: t('columns.newIssues'), width: 92, ...figure },
      }),
      column.accessor('event_count', {
        id: 'events',
        header: t('columns.events'),
        enableSorting: false,
        cell: ({ getValue }) => count(getValue()),
        meta: { label: t('columns.events'), width: 104, ...figure },
      }),
      column.accessor('last_event', {
        id: 'last_event',
        header: t('columns.lastEvent'),
        enableSorting: false,
        cell: ({ getValue }) => {
          const last = getValue();
          return last ? (
            <span
              className="text-fg-secondary"
              title={format.dateTime(new Date(last), 'dateTime')}
            >
              <RelativeTime at={last} />
            </span>
          ) : (
            <span className="text-fg-ghost">–</span>
          );
        },
        meta: { label: t('columns.lastEvent'), width: 128, ...figure },
      }),
    ]);
  }, [t, format]);

  const fields = useMemo<QueryField[]>(
    () => [
      {
        key: 'environment',
        label: t('environment'),
        icon: EnvironmentIcon,
        variant: 'options',
        multiple: false,
        options: environments.map((value) => ({ value, label: value })),
      },
    ],
    [environments, t],
  );

  const table = useDataTable({
    data: page?.items ?? NO_RELEASES,
    columns,
    rowCount: page?.total_count ?? 0,
    query,
    onQueryChange,
    getRowId: (release) => release.version,
    storageKey: 'releases',
    // The age under each version says most of it; the exact time is a click away.
    initialVisibility: { last_event: false },
  });

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      {/* Narrow, the bar takes a row and the window and columns the next. */}
      <div className="flex flex-wrap items-center gap-2">
        <QueryBar
          className="min-w-0 max-sm:basis-full"
          fields={fields}
          filters={query.filters}
          search={query.search}
          placeholder={t('search')}
          onChange={(next) =>
            onQueryChange((previous) => ({
              ...previous,
              filters: next.filters,
              search: next.search,
              pagination: { ...previous.pagination, pageIndex: 0 },
            }))
          }
        />
        <div className="flex min-w-0 flex-1 items-center gap-2 sm:flex-none">
          {period}
          <DataTableColumnsButton table={table} />
        </div>
      </div>
      <DataTable
        table={table}
        loading={!page || loading}
        className="min-h-0 flex-1"
        onRowClick={(row) =>
          router.push(releaseHref(projectId, row.original.version, scope))
        }
        empty={{ title: t('emptyWindow') }}
      />
    </div>
  );
}
