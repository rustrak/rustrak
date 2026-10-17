import { createFileRoute } from '@tanstack/react-router';
import { BellOff, Hourglass } from 'lucide-react';
import { useTransition } from 'react';
import { useTranslations } from 'use-intl';
import { getMonitor, listCheckIns } from '@/features/monitor/api/queries';
import { CheckInsTable } from '@/features/monitor/ui/components/check-ins-table';
import { CronSetupGuide } from '@/features/monitor/ui/components/cron-setup-guide';
import { MonitorActions } from '@/features/monitor/ui/components/monitor-actions';
import { MonitorConfigCard } from '@/features/monitor/ui/components/monitor-config';
import { MonitorEnvironments } from '@/features/monitor/ui/components/monitor-environments';
import { MonitorHealthBadge } from '@/features/monitor/ui/components/monitor-health';
import { getProject } from '@/features/project/api/queries';
import { translator } from '@/shared/i18n/intl';
import { searchPage } from '@/shared/lib/search-params';
import { Link } from '@/shared/ui/components/link';
import { LoadFailure } from '@/shared/ui/components/load-failure';
import { ScheduleText } from '@/shared/ui/components/schedule-text';
import { useRouter } from '@/shared/ui/hooks/use-router';

const PER_PAGE = 25;

export const Route = createFileRoute(
  '/_authenticated/projects/$id/crons/$slug/',
)({
  validateSearch: (search: Record<string, unknown>) => ({
    page: searchPage(search.page),
  }),
  loaderDeps: ({ search }) => ({ page: search.page ?? 1 }),
  loader: async ({ params, deps }) => {
    const projectId = Number.parseInt(params.id, 10);
    const [project, monitor, checkIns] = await Promise.all([
      getProject(projectId),
      getMonitor(projectId, params.slug),
      listCheckIns(projectId, params.slug, {
        page: deps.page,
        per_page: PER_PAGE,
      }),
    ]);
    return { project, monitor, checkIns };
  },
  head: ({ loaderData }) => ({
    meta: [
      {
        title: loaderData?.monitor.success
          ? translator('crons')('meta.detailTitle', {
              monitor: loaderData.monitor.data.name,
            })
          : translator('crons')('loadMonitorFailed'),
      },
    ],
  }),
  component: MonitorPage,
});

function MonitorPage() {
  const t = useTranslations('crons');
  const pageT = useTranslations('projectPages');
  const { id, slug } = Route.useParams();
  const page = Route.useSearch({ select: (search) => search.page ?? 1 });
  const { project, monitor, checkIns } = Route.useLoaderData();
  const router = useRouter();
  const [isPending, startTransition] = useTransition();
  const projectId = Number.parseInt(id, 10);

  if (!project.success) {
    return (
      <LoadFailure error={project.error} title={pageT('loadProjectFailed')} />
    );
  }
  if (!monitor.success) {
    return <LoadFailure error={monitor.error} title={t('loadMonitorFailed')} />;
  }
  if (!checkIns.success) {
    return (
      <LoadFailure
        error={checkIns.error}
        title={t('loadMonitorFailed')}
        notFoundOnMissing={false}
      />
    );
  }

  const m = monitor.data;
  const listHref = `/projects/${projectId}/crons`;
  const waiting = m.environments.length === 0;

  return (
    <div className="flex h-[calc(100vh-64px)] flex-col">
      <div className="flex shrink-0 flex-wrap items-start justify-between gap-4 border-b px-4 py-4 md:px-8 md:py-6">
        <div className="min-w-0">
          <nav className="mb-1 text-xs text-muted-foreground">
            <Link href={listHref} className="hover:text-foreground">
              {t('title')}
            </Link>
          </nav>
          <div className="flex flex-wrap items-center gap-3">
            <h1 className="truncate text-lg font-semibold">{m.name}</h1>
            <MonitorHealthBadge monitor={m} />
          </div>
          <p className="mt-1 text-sm text-muted-foreground">
            <ScheduleText
              schedule={m.config.schedule}
              timezone={m.config.timezone}
            />
          </p>
        </div>
        <MonitorActions projectId={projectId} monitor={m} />
      </div>

      <div className="flex-1 overflow-auto px-4 py-6 md:px-8">
        <div className="mx-auto grid max-w-6xl gap-6 lg:grid-cols-[minmax(0,1fr)_18rem]">
          <div className="min-w-0 space-y-8">
            {m.is_muted && (
              <div className="flex items-center gap-2 rounded-lg border bg-muted/40 px-4 py-3 text-sm text-muted-foreground">
                <BellOff className="size-4 shrink-0" />
                {t('detail.mutedBanner')}
              </div>
            )}

            {waiting ? (
              <section className="space-y-4">
                <div className="flex items-start gap-3 rounded-lg border bg-card p-4">
                  <Hourglass className="mt-0.5 size-5 shrink-0 text-muted-foreground" />
                  <div>
                    <h2 className="text-sm font-semibold">
                      {t('detail.waitingTitle')}
                    </h2>
                    <p className="mt-0.5 text-sm text-muted-foreground">
                      {t('detail.waitingDescription')}
                    </p>
                  </div>
                </div>
                <CronSetupGuide dsn={project.data.dsn} slug={m.slug} />
              </section>
            ) : (
              <section className="space-y-3">
                <h2 className="text-sm font-semibold">
                  {t('detail.environments')}
                </h2>
                <MonitorEnvironments monitor={m} />
              </section>
            )}

            <section className="space-y-3">
              <h2 className="text-sm font-semibold">{t('detail.checkIns')}</h2>
              <CheckInsTable
                page={checkIns.data}
                currentPage={page}
                showEnvironment={m.environments.length > 1}
                pending={isPending}
                onPageChange={(next) =>
                  startTransition(() => {
                    router.push(
                      `${listHref}/${encodeURIComponent(slug)}?page=${next}`,
                    );
                  })
                }
              />
            </section>

            {!waiting && (
              <section className="space-y-3">
                <h2 className="text-sm font-semibold">
                  {t('detail.instrument')}
                </h2>
                <CronSetupGuide dsn={project.data.dsn} slug={m.slug} />
              </section>
            )}
          </div>

          <aside className="space-y-4">
            <MonitorConfigCard monitor={m} />
          </aside>
        </div>
      </div>
    </div>
  );
}
