import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { getMonitor } from '@/features/monitor/api/queries';
import { MonitorForm } from '@/features/monitor/ui/components/monitor-form';
import { translator } from '@/shared/i18n/intl';
import { Link } from '@/shared/ui/components/link';
import { LoadFailure } from '@/shared/ui/components/load-failure';

export const Route = createFileRoute(
  '/_authenticated/projects/$id/crons/$slug/edit',
)({
  loader: async ({ params }) => ({
    monitor: await getMonitor(Number.parseInt(params.id, 10), params.slug),
  }),
  head: ({ loaderData }) => ({
    meta: [
      {
        title: loaderData?.monitor.success
          ? translator('crons')('meta.editTitle', {
              monitor: loaderData.monitor.data.name,
            })
          : translator('crons')('loadMonitorFailed'),
      },
    ],
  }),
  component: EditMonitorPage,
});

function EditMonitorPage() {
  const t = useTranslations('crons');
  const { id } = Route.useParams();
  const { monitor } = Route.useLoaderData();
  const projectId = Number.parseInt(id, 10);

  if (!monitor.success) {
    return <LoadFailure error={monitor.error} title={t('loadMonitorFailed')} />;
  }
  const detailHref = `/projects/${projectId}/crons/${encodeURIComponent(monitor.data.slug)}`;

  return (
    <div className="flex h-[calc(100vh-64px)] flex-col">
      <div className="shrink-0 border-b px-4 py-4 md:px-8 md:py-6">
        <nav className="mb-1 flex gap-1.5 text-xs text-muted-foreground">
          <Link
            href={`/projects/${projectId}/crons`}
            className="hover:text-foreground"
          >
            {t('title')}
          </Link>
          <span aria-hidden="true">/</span>
          <Link href={detailHref} className="hover:text-foreground">
            {monitor.data.name}
          </Link>
        </nav>
        <h1 className="text-lg font-semibold">
          {t('form.editTitle', { monitor: monitor.data.name })}
        </h1>
        <p className="mt-0.5 text-sm text-muted-foreground">
          {t('form.editDescription')}
        </p>
      </div>
      <div className="flex-1 overflow-auto px-4 py-6 md:px-8">
        <div className="mx-auto max-w-5xl">
          <MonitorForm projectId={projectId} monitor={monitor.data} />
        </div>
      </div>
    </div>
  );
}
