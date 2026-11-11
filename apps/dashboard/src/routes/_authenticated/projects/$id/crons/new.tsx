import { PageHeader } from '@rustrak/ui';
import { useQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import {
  MonitorForm,
  MonitorFormSkeleton,
} from '@/features/monitor/ui/components/monitor-form';
import { projectQueries } from '@/features/project/api/queries';
import { NotAuthorized } from '@/shared/ui/components/not-authorized';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { usePageTrail } from '@/shared/ui/hooks/use-page-trail';
import { usePermissions } from '@/shared/ui/hooks/use-permissions';

export const Route = createFileRoute('/_authenticated/projects/$id/crons/new')({
  pendingComponent: NewMonitorLoading,
  component: NewMonitorPage,
});

function NewMonitorPage() {
  const t = useTranslations('crons');
  const { id: projectId } = Route.useParams();
  const project = useQuery(projectQueries.detail(projectId)).data;
  const canCreate = usePermissions().can('monitors', 'create', projectId);
  usePageTrail([{ label: t('form.newTitle') }]);

  return (
    <div className="flex h-full flex-col">
      {project ? (
        <title>{t('meta.newTitle', { project: project.name })}</title>
      ) : null}
      <PageHeader
        title={t('form.newTitle')}
        summary={[t('form.newDescription')]}
      />
      <div className="flex-1 overflow-auto px-4 py-6 md:px-8">
        <div className="mx-auto max-w-5xl">
          {canCreate ? (
            <MonitorForm projectId={projectId} />
          ) : (
            <NotAuthorized />
          )}
        </div>
      </div>
    </div>
  );
}

function NewMonitorLoading() {
  const t = useTranslations('crons');
  return (
    <PageLoading>
      <div className="flex h-full flex-col">
        <PageHeader
          title={t('form.newTitle')}
          summary={[t('form.newDescription')]}
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
