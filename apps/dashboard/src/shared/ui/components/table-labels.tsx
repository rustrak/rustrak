import { type DataTableLabels, DataTableLabelsProvider } from '@rustrak/ui';
import { type ReactNode, useMemo } from 'react';
import { useLocale, useTranslations } from 'use-intl';

/**
 * Every table and query bar in the app speaks the reader's language: the
 * design system's words, filled from the `table` catalogue once, around the
 * whole authenticated app.
 */
export function TableLabels({ children }: { children: ReactNode }) {
  const t = useTranslations('table');
  const locale = useLocale();

  const labels = useMemo<Partial<DataTableLabels>>(
    () => ({
      locale,
      selected: (count) => t('selected', { count }),
      clearSelection: t('clearSelection'),
      selectionToolbar: t('selectionToolbar'),
      noResults: t('noResults'),
      noMatches: t('noMatches'),
      nothingYet: t('nothingYet'),
      clearFilters: t('clearFilters'),
      selectAllRows: t('selectAllPageRows'),
      selectRow: t('selectRow'),
      actions: t('actions'),
      rowActions: t('rowActions'),
      columns: t('columns'),
      chooseColumns: t('chooseColumns'),
      resizeColumn: t('resizeColumn'),
      filterValues: t('filterValues'),
      filterValuesPlaceholder: t('filterValuesPlaceholder'),
      nothingMatches: t('nothingMatches'),
      contains: t('contains'),
      filterText: t('filterText'),
      min: t('min'),
      max: t('max'),
      lowestFirst: t('lowestFirst'),
      highestFirst: t('highestFirst'),
      aToZ: t('aToZ'),
      zToA: t('zToA'),
      clearFilter: t('clearFilter'),
      hideColumn: t('hideColumn'),
      range: (first, last, total) => t('range', { first, last, total }),
      rowsPerPage: (size) => t('rowsPerPage', { size }),
      rowsOption: (size) => t('rowsOption', { size }),
      previousPage: t('previousPage'),
      nextPage: t('nextPage'),
      filterBy: t('filterBy'),
      suggestions: t('suggestions'),
      filterAndSearch: t('filterAndSearch'),
      addFilter: t('addFilter'),
      clearAll: t('clearAll'),
      hintNavigate: t('hintNavigate'),
      hintSelect: t('hintSelect'),
      hintClose: t('hintClose'),
      typeValueThen: t('typeValueThen'),
      removeFilter: (name) => t('removeFilter', { name }),
      searchFor: (text) => t('searchFor', { text }),
    }),
    [t, locale],
  );

  return (
    <DataTableLabelsProvider labels={labels}>
      {children}
    </DataTableLabelsProvider>
  );
}
