import { useTranslations } from 'use-intl';
import { getAutoCleanup } from '@/features/storage/api/queries';
import { AutoCleanup } from '@/features/storage/ui/components/auto-cleanup';
import { CleanupSkeleton } from '@/features/storage/ui/components/storage-skeletons';
import { LoadFailure } from '@/shared/ui/components/load-failure';
import { useAsync } from '@/shared/ui/hooks/use-async';

/** The automatic cleanup card, filling in on its own like the panels around it. */
export function AutoCleanupPanel() {
  const t = useTranslations('storage');
  const read = useAsync(() => getAutoCleanup(), []);

  if (read.state === 'pending') return <CleanupSkeleton />;
  if (!read.data.success) {
    return (
      <LoadFailure
        error={read.data.error}
        title={t('auto.loadFailed')}
        notFoundOnMissing={false}
      />
    );
  }
  return <AutoCleanup initial={read.data.data} />;
}
