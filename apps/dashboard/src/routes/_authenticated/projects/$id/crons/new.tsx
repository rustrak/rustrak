import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { MonitorForm } from '@/features/monitor/ui/components/monitor-form';
import { getProject } from '@/features/project/api/queries';
import { translator } from '@/shared/i18n/intl';
import { Link } from '@/shared/ui/components/link';
import { LoadFailure } from '@/shared/ui/components/load-failure';

export const Route = createFileRoute('/_authenticated/projects/$id/crons/new')({
  loader: async ({ params }) => ({
    project: await getProject(Number.parseInt(params.id, 10)),
  }),
  head: ({ loaderData }) => {
    if (!loaderData?.project.success) {
      return {
        meta: [{ title: translator('projectPages')('projectNotFound') }],
      };
    }
    return {
      meta: [
        {
          title: translator('crons')('meta.newTitle', {
            project: loaderData.project.data.name,
          }),
        },
      ],
    };
  },
  component: NewMonitorPage,
});

function NewMonitorPage() {
  const t = useTranslations('crons');
  const pageT = useTranslations('projectPages');
  const { id } = Route.useParams();
  const { project } = Route.useLoaderData();
  const projectId = Number.parseInt(id, 10);

  if (!project.success) {
    return (
      <LoadFailure error={project.error} title={pageT('loadProjectFailed')} />
    );
  }

  return (
    <div className="flex h-[calc(100vh-64px)] flex-col">
      <div className="shrink-0 border-b px-4 py-4 md:px-8 md:py-6">
        <nav className="mb-1 text-xs text-muted-foreground">
          <Link
            href={`/projects/${projectId}/crons`}
            className="hover:text-foreground"
          >
            {t('title')}
          </Link>
        </nav>
        <h1 className="text-lg font-semibold">{t('form.newTitle')}</h1>
        <p className="mt-0.5 text-sm text-muted-foreground">
          {t('form.newDescription')}
        </p>
      </div>
      <div className="flex-1 overflow-auto px-4 py-6 md:px-8">
        <div className="mx-auto max-w-5xl">
          <MonitorForm projectId={projectId} />
        </div>
      </div>
    </div>
  );
}
