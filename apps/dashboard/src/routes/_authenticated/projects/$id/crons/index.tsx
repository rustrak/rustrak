import { createFileRoute } from '@tanstack/react-router';
import { Plus, Timer } from 'lucide-react';
import { useTranslations } from 'use-intl';
import { listMonitors } from '@/features/monitor/api/queries';
import { CronSetupGuide } from '@/features/monitor/ui/components/cron-setup-guide';
import { MonitorsTable } from '@/features/monitor/ui/components/monitors-table';
import { getProject } from '@/features/project/api/queries';
import { translator } from '@/shared/i18n/intl';
import { Link } from '@/shared/ui/components/link';
import { LoadFailure } from '@/shared/ui/components/load-failure';
import { Button } from '@/shared/ui/components/shadcn/button';

export const Route = createFileRoute('/_authenticated/projects/$id/crons/')({
  loader: async ({ params }) => {
    const projectId = Number.parseInt(params.id, 10);
    const project = await getProject(projectId);
    if (!project.success) return { project, monitors: null };
    return { project, monitors: await listMonitors(projectId) };
  },
  head: ({ loaderData }) => {
    const t = translator('crons');
    if (!loaderData?.project.success) {
      return {
        meta: [{ title: translator('projectPages')('projectNotFound') }],
      };
    }
    const project = loaderData.project.data.name;
    return {
      meta: [
        { title: t('meta.listTitle', { project }) },
        {
          name: 'description',
          content: t('meta.listDescription', { project }),
        },
      ],
    };
  },
  component: CronsPage,
});

/** A sample slug and schedule for the snippets shown before any monitor exists. */
const SAMPLE_SLUG = 'my-job';
const SAMPLE_SCHEDULE = { type: 'crontab', value: '0 * * * *' } as const;

function CronsPage() {
  const t = useTranslations('crons');
  const pageT = useTranslations('projectPages');
  const { id } = Route.useParams();
  const { project: projectResult, monitors: monitorsResult } =
    Route.useLoaderData();
  const projectId = Number.parseInt(id, 10);

  if (!projectResult.success) {
    return (
      <LoadFailure
        error={projectResult.error}
        title={pageT('loadProjectFailed')}
      />
    );
  }
  if (monitorsResult === null) return null;
  if (!monitorsResult.success) {
    return (
      <LoadFailure
        error={monitorsResult.error}
        title={t('loadFailed')}
        notFoundOnMissing={false}
      />
    );
  }

  const project = projectResult.data;
  const monitors = monitorsResult.data;
  const newHref = `/projects/${projectId}/crons/new`;

  return (
    <div className="flex h-[calc(100vh-64px)] flex-col">
      <div className="flex w-full shrink-0 items-center justify-between gap-4 border-b px-4 py-4 md:px-8 md:py-6">
        <div>
          <h1 className="text-lg font-semibold">{t('title')}</h1>
          <p className="mt-0.5 text-sm text-muted-foreground">
            {t('subtitle')}
          </p>
        </div>
        {monitors.length > 0 && (
          <Button nativeButton={false} render={<Link href={newHref} />}>
            <Plus />
            {t('newMonitor')}
          </Button>
        )}
      </div>

      <div className="flex-1 overflow-auto px-4 py-4 md:px-8 md:py-6">
        {monitors.length === 0 ? (
          <div className="mx-auto max-w-3xl py-6">
            <div className="mb-6 flex flex-col items-center text-center">
              <div className="mb-4 flex size-12 items-center justify-center rounded-full bg-primary/10">
                <Timer className="size-6 text-primary" />
              </div>
              <h2 className="text-lg font-semibold">{t('empty.title')}</h2>
              <p className="mt-1 max-w-xl text-sm text-muted-foreground">
                {t('empty.description')}
              </p>
            </div>
            <CronSetupGuide
              dsn={project.dsn}
              slug={SAMPLE_SLUG}
              schedule={SAMPLE_SCHEDULE}
            />
            <p className="mt-3 text-xs text-muted-foreground">
              {t('empty.upsertNote')}
            </p>
            <div className="mt-8 flex items-center justify-center gap-3 border-t pt-6 text-sm text-muted-foreground">
              {t('empty.orManual')}
              <Button
                variant="outline"
                size="sm"
                nativeButton={false}
                render={<Link href={newHref} />}
              >
                {t('empty.manual')}
              </Button>
            </div>
          </div>
        ) : (
          <MonitorsTable projectId={projectId} monitors={monitors} />
        )}
      </div>
    </div>
  );
}
