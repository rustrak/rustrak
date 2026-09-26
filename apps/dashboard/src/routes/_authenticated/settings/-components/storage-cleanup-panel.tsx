import { useQuery } from '@tanstack/react-query';
import { useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import { StorageCleanup } from '@/features/storage/ui/components/storage-cleanup';
import { CleanupSkeleton } from '@/features/storage/ui/components/storage-skeletons';
import { LoadFailure } from '@/shared/ui/components/load-failure';

/**
 * Cleanup panel. Uses the lightweight projects list (id + name) for its scope
 * selector instead of waiting on the heavy per-project storage aggregation, so
 * it can stream in early.
 */
export function StorageCleanupPanel() {
  const t = useTranslations('settings');
  // Fetch every project in one shot (the API applies no hard page-size cap) so
  // the scope selector never silently drops projects.
  const { data: result } = useQuery(projectQueries.list({ per_page: 10000 }));

  if (!result) return <CleanupSkeleton />;

  if (!result.success) {
    return (
      <LoadFailure
        error={result.error}
        title={t('loadProjectsFailed')}
        notFoundOnMissing={false}
      />
    );
  }

  return (
    <StorageCleanup
      projects={result.data.items.map((p) => ({ id: p.id, name: p.name }))}
    />
  );
}
