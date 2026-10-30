import { PageHeader } from '@rustrak/ui';
import { noop, useQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { MonitorForm } from '@/features/monitor/ui/components/monitor-form';
import { projectQueries } from '@/features/project/api/queries';
import { usePageTrail } from '@/shared/ui/hooks/use-page-trail';

export const Route = createFileRoute('/_authenticated/projects/$id/crons/new')({
  // Only for the title, which waits for it; the form does not.
  loader: ({ context: { queryClient }, params }) => {
    void queryClient
      .query(projectQueries.detail(Number.parseInt(params.id, 10)))
      .catch(noop);
  },
  component: NewMonitorPage,
});

function NewMonitorPage() {
  const t = useTranslations('crons');
  const { id } = Route.useParams();
  const projectId = Number.parseInt(id, 10);
  const project = useQuery(projectQueries.detail(projectId)).data;
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
          <MonitorForm projectId={projectId} />
        </div>
      </div>
    </div>
  );
}
