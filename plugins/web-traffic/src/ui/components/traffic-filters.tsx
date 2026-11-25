import {
  PeriodFilter,
  usePluginFetch,
  usePluginSearch,
  usePluginTranslations,
} from '@rustrak/plugin/ui';
import { QueryBar, type QueryField } from '@rustrak/ui';
import { useMemo } from 'react';
import {
  DEFAULT_PERIOD,
  DIMENSIONS,
  type Dimension,
  filterOf,
  PERIODS,
  type Row,
} from '../model';

/**
 * The bar every page of the report is narrowed with: any dimension, offered
 * with the values the window has, and the window itself. Free text is a page.
 */
export function TrafficFilters() {
  const t = usePluginTranslations();
  const read = usePluginFetch();
  const [search, setSearch] = usePluginSearch();
  const filter = filterOf(search);

  const fields = useMemo<QueryField[]>(
    () =>
      DIMENSIONS.map((d) => ({
        key: d,
        label: t(`dimension.${d}`),
        variant: 'options' as const,
        multiple: false,
        loadOptions: async () =>
          (
            await read<Row[]>('/top', {
              by: d,
              period: filter.period,
              limit: 50,
            })
          ).map((row) => ({ value: row.value, label: row.value })),
      })),
    [t, read, filter.period],
  );

  return (
    <div className="flex flex-wrap items-center gap-2">
      <QueryBar
        className="min-w-0 max-sm:basis-full"
        label={t('filters.label')}
        placeholder={t('filters.placeholder')}
        fields={fields}
        filters={DIMENSIONS.filter((d) => search[d]).map((d) => ({
          id: d,
          value: [search[d]!],
        }))}
        search=""
        onChange={(next) => {
          const set: Record<string, string | undefined> = {};
          for (const d of DIMENSIONS) set[d] = undefined;
          for (const f of next.filters) {
            const value = Array.isArray(f.value) ? f.value[0] : f.value;
            if (typeof value === 'string') set[f.id as Dimension] = value;
          }
          // Typed text with no `key:` is a page, as the placeholder says.
          set.path = next.search.trim() || set.path;
          setSearch(set);
        }}
      />
      <PeriodFilter
        value={filter.period}
        choices={PERIODS}
        onChange={(period) =>
          setSearch({ period: period === DEFAULT_PERIOD ? undefined : period })
        }
      />
    </div>
  );
}
