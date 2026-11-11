import { createContext, type ReactNode, useContext, useMemo } from 'react';

/**
 * Every word the table and the query bar say on their own.
 *
 * The package has no i18n of its own and should not: the app already has a
 * catalogue and a reader's language. So the words come in through context,
 * once, around the screen -- English when nothing is provided, which is what
 * Storybook reads -- and every part of the table reads the same set, so a
 * screen cannot end up half translated.
 */
export interface DataTableLabels {
  /** BCP 47 tag the figures are formatted in. */
  locale: string;
  selected: (count: number) => string;
  clearSelection: string;
  selectionToolbar: string;
  noResults: string;
  noMatches: string;
  nothingYet: string;
  clearFilters: string;
  selectAllRows: string;
  selectRow: string;
  actions: string;
  rowActions: string;
  columns: string;
  chooseColumns: string;
  resizeColumn: string;
  filterValues: string;
  filterValuesPlaceholder: string;
  nothingMatches: string;
  contains: string;
  filterText: string;
  min: string;
  max: string;
  lowestFirst: string;
  highestFirst: string;
  aToZ: string;
  zToA: string;
  clearFilter: string;
  hideColumn: string;
  /** "1–50 of 230", with the figures already formatted. */
  range: (first: string, last: string, total: string) => string;
  rowsPerPage: (size: number) => string;
  rowsOption: (size: number) => string;
  previousPage: string;
  nextPage: string;
  filterBy: string;
  suggestions: string;
  filterAndSearch: string;
  addFilter: string;
  clearAll: string;
  /** The suggestions' key hints: "↑↓ navigate", "⏎ select", "esc closes". */
  hintNavigate: string;
  hintSelect: string;
  hintClose: string;
  /** Before the ⏎ key, while a free-text value is being typed. */
  typeValueThen: string;
  /** Names a chip's remove button: "Remove Level filter". */
  removeFilter: (name: string) => string;
  /** The free-text suggestion: "Search “timeout”". */
  searchFor: (text: string) => string;
}

const ENGLISH_LABELS: DataTableLabels = {
  locale: 'en-US',
  selected: (count) => `${count} selected`,
  clearSelection: 'Clear',
  selectionToolbar: 'Actions for the selected rows',
  noResults: 'No results',
  noMatches: 'Nothing matches the current filters.',
  nothingYet: 'There is nothing here yet.',
  clearFilters: 'Clear filters',
  selectAllRows: 'Select all rows on this page',
  selectRow: 'Select row',
  actions: 'Actions',
  rowActions: 'Row actions',
  columns: 'Columns',
  chooseColumns: 'Choose columns',
  resizeColumn: 'Resize column',
  filterValues: 'Filter values',
  filterValuesPlaceholder: 'Filter values…',
  nothingMatches: 'Nothing matches',
  contains: 'Contains…',
  filterText: 'Filter text',
  min: 'Min',
  max: 'Max',
  lowestFirst: 'Lowest first',
  highestFirst: 'Highest first',
  aToZ: 'A to Z',
  zToA: 'Z to A',
  clearFilter: 'Clear filter',
  hideColumn: 'Hide column',
  range: (first, last, total) => `${first}–${last} of ${total}`,
  rowsPerPage: (size) => `Rows ${size}`,
  rowsOption: (size) => `${size} rows`,
  previousPage: 'Previous page',
  nextPage: 'Next page',
  filterBy: 'Filter by',
  suggestions: 'Suggestions',
  filterAndSearch: 'Filter and search',
  addFilter: 'Add a filter…',
  clearAll: 'Clear filters and search',
  hintNavigate: 'navigate',
  hintSelect: 'select',
  hintClose: 'esc closes',
  typeValueThen: 'Type a value, then',
  removeFilter: (name) => `Remove ${name} filter`,
  searchFor: (text) => `Search “${text}”`,
};

const LabelsContext = createContext<DataTableLabels>(ENGLISH_LABELS);

/** Gives every table and query bar inside it the reader's words. */
export function DataTableLabelsProvider({
  labels,
  children,
}: {
  /** Whatever is left out stays English. */
  labels: Partial<DataTableLabels>;
  children: ReactNode;
}) {
  const value = useMemo(() => ({ ...ENGLISH_LABELS, ...labels }), [labels]);
  return (
    <LabelsContext.Provider value={value}>{children}</LabelsContext.Provider>
  );
}

DataTableLabelsProvider.displayName = 'DataTableLabelsProvider';

export function useDataTableLabels(): DataTableLabels {
  return useContext(LabelsContext);
}
