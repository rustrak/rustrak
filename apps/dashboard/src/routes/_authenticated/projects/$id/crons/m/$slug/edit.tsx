import { PageHeader, Skeleton } from '@rustrak/ui';
import { noop, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { monitorQueries } from '@/features/monitor/api/queries';
import { monitorHref } from '@/features/monitor/lib/href';
import {
  MonitorForm,
  MonitorFormSkeleton,
} from '@/features/monitor/ui/components/monitor-form';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { usePageTrail } from '@/shared/ui/hooks/use-page-trail';

export const Route = createFileRoute(
  '/_authenticated/projects/$id/crons/m/$slug/edit',
)({
  loader: ({ context: { queryClient }, params }) => {
    void queryClient
      .query(monitorQueries.detail(Number.parseInt(params.id, 10), params.slug))
      .catch(noop);
  },
  pendingComponent: EditMonitorLoading,
  component: EditMonitorPage,
});

function EditMonitorPage() {
  const t = useTranslations('crons');
  const { id, slug } = Route.useParams();
  const projectId = Number.parseInt(id, 10);
  const { data: monitor } = useSuspenseQuery(
    monitorQueries.detail(projectId, slug),
  );
  usePageTrail([
    { label: monitor.name, href: monitorHref(projectId, monitor.slug) },
    { label: t('actions.edit') },
  ]);

  return (
    <div className="flex h-full flex-col">
      <title>{t('meta.editTitle', { monitor: monitor.name })}</title>
      <PageHeader
        title={t('form.editTitle', { monitor: monitor.name })}
        summary={[t('form.editDescription')]}
      />
      <div className="flex-1 overflow-auto px-4 py-6 md:px-8">
        <div className="mx-auto max-w-5xl">
          <MonitorForm projectId={projectId} monitor={monitor} />
        </div>
      </div>
    </div>
  );
}

function EditMonitorLoading() {
  const t = useTranslations('crons');
  return (
    <PageLoading>
      <div className="flex h-full flex-col">
        <PageHeader
          title={<Skeleton className="h-7 w-64 max-w-full" />}
          summary={[t('form.editDescription')]}
        />
        <div className="flex-1 overflow-auto px-4 py-6 md:px-8">
          <div className="mx-auto max-w-5xl">
            <MonitorFormSkeleton />
          </div>
        </div>
      </div>
    </PageLoading>
  );
}
