import { StorageIcon } from '@rustrak/ui';
import { noop } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { storageQueries } from '@/features/storage/api/queries';
import { StorageViewSkeleton } from '@/features/storage/ui/components/storage-skeletons';
import { StorageView } from '@/features/storage/ui/components/storage-view';
import { session } from '@/shared/api/session';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { SettingsEmpty } from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
import { useSessionUser } from '@/shared/ui/hooks/use-session-user';

export const Route = createFileRoute('/_authenticated/settings/storage')({
  /**
   * Only for an admin: anyone else gets a 403 from every read here, under a
   * "not authorised" panel that already explains why.
   *
   * The retention is what the view suspends on: it answers at once and brings
   * up the header and its actions. The figures can take seconds and fill in
   * their own panels.
   */
  loader: ({ context: { queryClient } }) => {
    const answer = session.peek();
    if (answer?.state === 'authenticated' && answer.user.role === 'admin') {
      void queryClient.query(storageQueries.retention()).catch(noop);
      void queryClient.query(storageQueries.summary()).catch(noop);
      void queryClient.query(storageQueries.projects()).catch(noop);
      void queryClient.query(storageQueries.age()).catch(noop);
      void queryClient.query(storageQueries.compaction()).catch(noop);
    }
  },
  pendingComponent: StorageLoading,
  component: StoragePage,
});

function StoragePage() {
  const t = useTranslations('settings');
  // Resolved by the gate above. The "we could not ask" branch that used to sit
  // between these two is `_authenticated`'s now: telling a visitor "Not
  // authorized" when the truth is "we could not reach the API to find out" is
  // a lie about their permissions, and it is told in one place or none.
  const user = useSessionUser();

  // Guard: storage usage and cleanup are instance-admin only.
  if (user.role !== 'admin') {
    return (
      <>
        <title>{t('storage.meta.title')}</title>
        <SettingsHeader
          title={t('storage.title')}
          description={t('storage.subtitle')}
        />
        <div className="rounded-2xl border border-border-subtle bg-surface">
          <SettingsEmpty
            icon={<StorageIcon size="2xl" />}
            title={t('notAuthorized')}
            description={t('storage.notAuthorizedDescription')}
          />
        </div>
      </>
    );
  }

  // The header is the view's: its actions need the view's state.
  return (
    <>
      <title>{t('storage.meta.title')}</title>
      <StorageView
        title={t('storage.title')}
        subtitle={t('storage.subtitle')}
      />
    </>
  );
}

/** Only an admin waits here, on the retention `StorageView` suspends on. */
function StorageLoading() {
  const t = useTranslations('settings');
  return (
    <PageLoading>
      <title>{t('storage.meta.title')}</title>
      <StorageViewSkeleton
        title={t('storage.title')}
        subtitle={t('storage.subtitle')}
      />
    </PageLoading>
  );
}
