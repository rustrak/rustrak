import type { IssueFilter } from '@rustrak/client';
import { useTranslations } from 'use-intl';
import {
  Tabs,
  TabsList,
  TabsTrigger,
} from '@/shared/ui/components/shadcn/tabs';

const FILTERS: readonly { value: IssueFilter; key: string }[] = [
  { value: 'open', key: 'filters.open' },
  { value: 'resolved', key: 'filters.resolved' },
  { value: 'muted', key: 'filters.muted' },
  { value: 'all', key: 'filters.all' },
];

/**
 * The status filter, and only that.
 *
 * The batch actions used to live here, on a line that appeared the moment a
 * checkbox was ticked and pushed the whole table down by its own height. They
 * are now inside the table's own header row, where the columns make room for
 * them instead of the page doing it.
 */
export function IssueFilters({
  currentFilter,
  onFilterChange,
  disabled,
}: {
  currentFilter: IssueFilter;
  onFilterChange: (filter: IssueFilter) => void;
  disabled: boolean;
}) {
  const t = useTranslations('issues');
  return (
    <div className="mb-4 shrink-0">
      <Tabs
        value={currentFilter}
        onValueChange={(value) => {
          const filter = FILTERS.find((f) => f.value === value);
          if (filter) onFilterChange(filter.value);
        }}
      >
        <TabsList>
          {FILTERS.map((filter) => (
            <TabsTrigger
              key={filter.value}
              value={filter.value}
              disabled={disabled}
            >
              {t(filter.key)}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>
    </div>
  );
}
