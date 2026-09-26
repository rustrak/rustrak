import { createFileRoute } from '@tanstack/react-router';
import { ShieldX } from 'lucide-react';
import { useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import { storageQueries } from '@/features/storage/api/queries';
import { SourceMapGc } from '@/features/storage/ui/components/source-map-gc';
import { StorageProjectsTable } from '@/features/storage/ui/components/storage-projects-table';
import { StorageSummaryCards } from '@/features/storage/ui/components/storage-summary-cards';
import { session } from '@/shared/api/session';
import { translator } from '@/shared/i18n/intl';
import { Card, CardContent } from '@/shared/ui/components/shadcn/card';
import { useSessionUser } from '@/shared/ui/hooks/use-session-user';
import { StorageCleanupPanel } from './-components/storage-cleanup-panel';

export const Route = createFileRoute('/_authenticated/settings/storage')({
  head: () => {
    const t = translator('settings');
    return {
      meta: [
        { title: t('storage.meta.title') },
        { name: 'description', content: t('storage.meta.description') },
      ],
    };
  },
  // Started here rather than when each panel mounts, and not awaited: every
  // panel still appears when its own answer lands.
  loader: ({ context: { queryClient } }) => {
    const answer = session.peek();
    if (answer?.state !== 'authenticated' || answer.user.role !== 'admin') {
      return;
    }
    void queryClient.prefetchQuery(storageQueries.summary());
    void queryClient.prefetchQuery(storageQueries.projects());
    void queryClient.prefetchQuery(projectQueries.list({ per_page: 10000 }));
  },
  component: StoragePage,
});

function PageHeader() {
  const t = useTranslations('settings');

  return (
    <div className="mb-6 md:mb-8">
      <h1 className="text-xl md:text-2xl font-extrabold tracking-tight">
        {t('storage.title')}
      </h1>
      <p className="text-muted-foreground mt-1">{t('storage.subtitle')}</p>
    </div>
  );
}

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
        <PageHeader />
        <Card className="border-dashed">
          <CardContent className="flex flex-col items-center justify-center py-12 text-center">
            <ShieldX className="size-12 text-muted-foreground/50 mb-4" />
            <p className="font-semibold">{t('notAuthorized')}</p>
            <p className="text-muted-foreground mt-1 text-sm max-w-sm">
              {t('storage.notAuthorizedDescription')}
            </p>
          </CardContent>
        </Card>
      </>
    );
  }

  // The page shell + header render immediately. Each data-heavy section owns
  // its own query and its own skeleton, so they fill in independently instead
  // of the whole page blocking on the slowest one.
  return (
    <>
      <PageHeader />
      <StorageSummaryCards />
      <StorageProjectsTable />
      <div className="space-y-6">
        <StorageCleanupPanel />
        <SourceMapGc />
      </div>
    </>
  );
}
