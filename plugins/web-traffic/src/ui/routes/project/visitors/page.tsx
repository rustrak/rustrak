import {
  definePage,
  type PageProps,
  useFormatter,
  usePluginApi,
  usePluginNavigate,
  usePluginSearch,
  usePluginTranslations,
} from '@rustrak/plugin/ui';
import {
  createDataTableColumnHelper,
  DataTable,
  type DataTableQuery,
  useDataTable,
} from '@rustrak/ui';
import { useMemo, useState } from 'react';
import { useRegionName } from '../../../components/region';
import { TrafficPage } from '../../../components/traffic-page';
import { duration, filterOf, periodOf, type Visitor } from '../../../model';

export const config = definePage({
  requires: 'visitors:read',
  sidebar: { label: 'nav.visitors', icon: 'team', rank: 2 },
});

const column = createDataTableColumnHelper<Visitor>();
const NONE: Visitor[] = [];

/** Who visited in the window, most recent first; each opens their visit. */
export default function Visitors(_: PageProps<'project'>) {
  const t = usePluginTranslations();
  const format = useFormatter();
  const go = usePluginNavigate();
  const [search] = usePluginSearch();
  const filter = filterOf(search);
  const visitors = usePluginApi<Visitor[]>('/visitors', {
    ...filter,
    limit: 200,
  });
  const regionName = useRegionName();

  const columns = useMemo(
    () =>
      column.columns([
        column.accessor('entry', {
          id: 'visitor',
          header: t('visitors.visit'),
          enableSorting: false,
          enableHiding: false,
          cell: ({ row }) => {
            const v = row.original;
            return (
              <div className="flex min-w-0 flex-col gap-0.5 py-1">
                <span className="truncate font-medium text-value text-fg">
                  {v.entry}
                </span>
                <span className="truncate text-meta text-fg-subtle">
                  {[
                    v.country && regionName(v.country),
                    v.browser,
                    v.os,
                    v.device,
                  ]
                    .filter(Boolean)
                    .join(' · ')}
                </span>
              </div>
            );
          },
          meta: { label: t('visitors.visit') },
        }),
        column.accessor('referrer', {
          id: 'referrer',
          header: t('ranking.referrer'),
          enableSorting: false,
          cell: ({ getValue }) =>
            getValue() ?? <span className="text-fg-ghost">–</span>,
          meta: { label: t('ranking.referrer'), width: 180 },
        }),
        column.accessor('pageviews', {
          id: 'pageviews',
          header: t('overview.pageviews'),
          enableSorting: false,
          cell: ({ getValue }) => format.number(getValue()),
          meta: { label: t('overview.pageviews'), width: 110, align: 'end' },
        }),
        column.accessor('events', {
          id: 'events',
          header: t('top.events'),
          enableSorting: false,
          cell: ({ getValue }) => format.number(getValue()),
          meta: { label: t('top.events'), width: 90, align: 'end' },
        }),
        column.display({
          id: 'duration',
          header: t('overview.duration'),
          cell: ({ row }) => duration(row.original.last - row.original.first),
          meta: { label: t('overview.duration'), width: 100, align: 'end' },
        }),
        column.accessor('last', {
          id: 'last',
          header: t('visitors.last'),
          enableSorting: false,
          cell: ({ getValue }) =>
            format.relativeTime(new Date(getValue() * 1000)),
          meta: { label: t('visitors.last'), width: 130, align: 'end' },
        }),
      ]),
    [t, format, regionName],
  );

  const [query, setQuery] = useState<DataTableQuery>({
    sorting: [],
    filters: [],
    search: '',
    pagination: { pageIndex: 0, pageSize: 200 },
  });
  const table = useDataTable({
    data: visitors.data ?? NONE,
    columns,
    rowCount: visitors.data?.length ?? 0,
    query,
    onQueryChange: (update) => setQuery(update),
    getRowId: (v) => v.id,
    storageKey: 'web-traffic:visitors',
  });

  return (
    <TrafficPage
      fill
      title={t('visitors.title')}
      summary={[
        visitors.data
          ? t('visitors.count', { count: visitors.data.length })
          : null,
        t(`period.${periodOf(search)}`),
      ]}
    >
      <DataTable
        className="flex-1"
        table={table}
        loading={visitors.isPending}
        empty={{ title: t('top.none'), description: t('visitors.noneHint') }}
        onRowClick={(row) => go(`visitors/${row.original.id}`, search)}
      />
    </TrafficPage>
  );
}
