import {
  type PageProps,
  useFormatter,
  usePluginApi,
  usePluginNavigate,
  usePluginSearch,
  usePluginTrail,
  usePluginTranslations,
} from '@rustrak/plugin/ui';
import {
  createDataTableColumnHelper,
  DataTable,
  type DataTableQuery,
  Input,
  useDataTable,
} from '@rustrak/ui';
import { type ReactNode, useMemo, useState } from 'react';
import { useRegionName } from '../../../../components/region';
import { TrafficPage } from '../../../../components/traffic-page';
import {
  duration,
  eventPage,
  filterOf,
  MAX_ROWS,
  narrows,
  periodOf,
  RANKINGS,
  type Ranking,
  type Row,
  share,
  visitLength,
} from '../../../../model';

const column = createDataTableColumnHelper<Row>();
const NO_ROWS: Row[] = [];

/** Every value of one dimension over the window, searchable. */
export default function Breakdown({
  params,
  inSection = false,
  children,
}: PageProps<'project'> & {
  /** Drawn as a section's own page (Events): it names no crumb of its own. */
  inSection?: boolean;
  /** Above the table: a chart of what it lists. */
  children?: ReactNode;
}) {
  const t = usePluginTranslations();
  const format = useFormatter();
  const go = usePluginNavigate();
  const [search, setSearch] = usePluginSearch();
  const by = (RANKINGS as readonly string[]).includes(params.by ?? '')
    ? (params.by as Ranking)
    : 'path';
  const filter = filterOf(search);
  const rows = usePluginApi<Row[]>('/top', {
    by,
    ...filter,
    q: search.q,
    limit: MAX_ROWS,
  });
  const regionName = useRegionName();
  const events = by === 'event';
  const total = (rows.data ?? []).reduce(
    (sum, r) => sum + (events ? r.count : r.visitors),
    0,
  );

  const columns = useMemo(
    () =>
      column.columns([
        column.accessor('value', {
          id: 'value',
          header: t(`ranking.${by}`),
          enableSorting: false,
          enableHiding: false,
          cell: ({ getValue }) => (
            <span className="truncate text-fg">
              {by === 'country' ? regionName(getValue()) : getValue()}
            </span>
          ),
          meta: { label: t(`ranking.${by}`) },
        }),
        column.accessor('visitors', {
          id: 'visitors',
          header: t('overview.visitors'),
          enableSorting: false,
          cell: ({ getValue }) => format.number(getValue()),
          meta: { label: t('overview.visitors'), width: 120, align: 'end' },
        }),
        column.accessor('count', {
          id: 'count',
          header: t(events ? 'top.count' : 'overview.pageviews'),
          enableSorting: false,
          cell: ({ getValue }) => format.number(getValue()),
          meta: { label: t('overview.pageviews'), width: 120, align: 'end' },
        }),
        // A visit's bounce and length say nothing about an event.
        ...(events
          ? []
          : [
              column.display({
                id: 'bounce',
                header: t('overview.bounce'),
                cell: ({ row }) => {
                  const pct = share(
                    row.original.bounces ?? 0,
                    row.original.visitors,
                  );
                  return pct === null ? '–' : `${pct}%`;
                },
                meta: { label: t('overview.bounce'), width: 110, align: 'end' },
              }),
              column.display({
                id: 'duration',
                header: t('overview.duration'),
                cell: ({ row }) => {
                  const length = visitLength({
                    duration: row.original.duration ?? 0,
                    visitors: row.original.visitors,
                  });
                  return length === null ? '–' : duration(length);
                },
                meta: {
                  label: t('overview.duration'),
                  width: 120,
                  align: 'end',
                },
              }),
            ]),
        column.display({
          id: 'share',
          header: t('breakdown.share'),
          cell: ({ row }) => {
            const part = events ? row.original.count : row.original.visitors;
            return total === 0
              ? '–'
              : format.number(part / total, {
                  style: 'percent',
                  maximumFractionDigits: 1,
                });
          },
          meta: { label: t('breakdown.share'), width: 100, align: 'end' },
        }),
      ]),
    [t, format, by, events, total, regionName],
  );

  const [query, setQuery] = useState<DataTableQuery>({
    sorting: [],
    filters: [],
    search: '',
    pagination: { pageIndex: 0, pageSize: MAX_ROWS },
  });
  const table = useDataTable({
    data: rows.data ?? NO_ROWS,
    columns,
    rowCount: rows.data?.length ?? 0,
    query,
    onQueryChange: (update) => setQuery(update),
    getRowId: (row) => row.value,
  });

  usePluginTrail(inSection ? [] : [{ label: t(`ranking.${by}`) }]);

  const dimension = narrows(by);
  return (
    <TrafficPage
      fill
      title={t(`ranking.${by}`)}
      summary={[t(`period.${periodOf(search)}`)]}
    >
      {children}
      {/* A dimension's full list is searched; the events, few and named
          by the site itself, are read at a glance. */}
      {inSection ? null : (
        <Input
          aria-label={t('breakdown.search')}
          placeholder={t('breakdown.search')}
          value={search.q ?? ''}
          onChange={(e) => setSearch({ q: e.target.value || undefined })}
          className="max-w-sm shrink-0"
        />
      )}
      <DataTable
        className="flex-1"
        table={table}
        loading={rows.isPending}
        empty={{ title: t('top.none') }}
        onRowClick={(row) => {
          const { q: _, ...rest } = search;
          if (events) go(eventPage(row.original.value), rest);
          else if (dimension)
            go('', { ...rest, [dimension]: row.original.value });
        }}
      />
    </TrafficPage>
  );
}
