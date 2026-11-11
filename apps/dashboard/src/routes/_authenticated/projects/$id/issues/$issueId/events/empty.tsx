import { PageHeader, Skeleton } from '@rustrak/ui';
import { noop, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { issueQueries } from '@/features/issue/api/queries';
import { EmptyState } from '@/shared/ui/components/empty-state';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { usePageTrail } from '@/shared/ui/hooks/use-page-trail';

export const Route = createFileRoute(
  '/_authenticated/projects/$id/issues/$issueId/events/empty',
)({
  loader: ({ context: { queryClient }, params }) => {
    void queryClient
      .query(issueQueries.detail(params.id, params.issueId))
      .catch(noop);
  },
  pendingComponent: EmptyEventsLoading,
  component: EmptyEventsPage,
});

function EmptyEventsPage() {
  const t = useTranslations('projectPages');
  const { id, issueId } = Route.useParams();
  const { data: issue } = useSuspenseQuery(issueQueries.detail(id, issueId));
  usePageTrail([{ label: issue.short_id, mono: true }]);

  return (
    <div className="flex h-full flex-col">
      <title>{t('event.meta.noEvents', { issue: issue.title })}</title>
      <PageHeader title={issue.title} />
      <EmptyState
        title={t('event.noEventsYet')}
        description={t('event.noEventsDescription')}
      />
    </div>
  );
}

function EmptyEventsLoading() {
  return (
    <PageLoading>
      <div className="flex h-full flex-col">
        <PageHeader title={<Skeleton className="h-7 w-80 max-w-full" />} />
        {/* Not the empty state: nothing has said there are no events yet. */}
        <div className="flex flex-1 flex-col items-center justify-center gap-2 px-page-gutter py-16">
          <Skeleton shape="circle" className="mb-2 size-8" />
          <Skeleton className="h-4 w-48" />
          <Skeleton className="h-3 w-72 max-w-full" />
        </div>
      </div>
    </PageLoading>
  );
}
