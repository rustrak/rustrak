import { ChevronLeft, ChevronRight } from 'lucide-react';
import { useTranslations } from 'use-intl';
import { Button } from '@/shared/ui/components/shadcn/button';

/**
 * The footer of a paginated table: what is on screen, and the way to the rest.
 *
 * Takes the raw counts rather than a pre-built range because the lists that
 * use it derived `startIndex`/`endIndex` identically and one of them
 * getting the arithmetic wrong would be silent.
 *
 * Renders nothing when there is no page to move to. `total_pages` of 0 is an
 * empty result set, where "Page 1 of 0" is worse than no footer.
 */
export function TablePagination({
  currentPage,
  totalPages,
  totalCount,
  perPage,
  disabled,
  onPageChange,
}: {
  currentPage: number;
  totalPages: number;
  totalCount: number;
  perPage: number;
  disabled: boolean;
  onPageChange: (page: number) => void;
}) {
  const t = useTranslations('table');

  if (totalPages <= 0) return null;

  const startIndex = (currentPage - 1) * perPage + 1;
  const endIndex = Math.min(currentPage * perPage, totalCount);

  return (
    <div className="shrink-0 flex flex-col sm:flex-row items-center justify-between gap-2 pt-4">
      <span className="text-sm text-muted-foreground">
        {totalCount > 0
          ? t('showingRange', {
              start: startIndex,
              end: endIndex,
              total: totalCount,
            })
          : t('noResults')}
      </span>

      <div className="flex items-center gap-2">
        <Button
          variant="outline"
          size="sm"
          aria-label={t('previousPage')}
          onClick={() => onPageChange(currentPage - 1)}
          disabled={currentPage <= 1 || disabled}
        >
          <ChevronLeft className="size-4" aria-hidden="true" />
        </Button>

        <span className="text-sm px-2">
          {t('pageOf', { current: currentPage, total: totalPages })}
        </span>

        <Button
          variant="outline"
          size="sm"
          aria-label={t('nextPage')}
          onClick={() => onPageChange(currentPage + 1)}
          disabled={currentPage >= totalPages || disabled}
        >
          <ChevronRight className="size-4" aria-hidden="true" />
        </Button>
      </div>
    </div>
  );
}
