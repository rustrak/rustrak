import type { CleanupStatus } from '@rustrak/client';
import { Button, ConfigureIcon, DeleteIcon } from '@rustrak/ui';
import {
  useQuery,
  useQueryClient,
  useSuspenseQuery,
} from '@tanstack/react-query';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslations } from 'use-intl';
import { storageQueries } from '@/features/storage/api/queries';
import {
  KINDS,
  nextReadIn,
  summarize,
} from '@/features/storage/model/retention';
import { LoadFailure } from '@/shared/ui/components/load-failure';
import { SettingSection } from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
import { useCleanupJob } from '../hooks/use-cleanup-job';
import { AgeChart } from './age-chart';
import { CompactionNotice } from './compaction-notice';
import { DeleteDialog } from './delete-dialog';
import { ProjectRetentionDialog } from './project-retention-dialog';
import { RetentionDialog } from './retention-dialog';
import { CleanupLine, StorageOverview } from './storage-overview';
import { StorageProjects } from './storage-projects';
import {
  ChartSkeleton,
  LineSkeleton,
  OverviewSkeleton,
  ProjectsSkeleton,
} from './storage-skeletons';

/**
 * The storage page: what is held, how old it is, and which project holds
 * it. The header's two actions change retention and delete data now; a
 * project's own retention is changed from its row.
 *
 * The header and its actions need only the retention, which this suspends on
 * (the route's `pendingComponent` is `StorageViewSkeleton` meanwhile); the
 * figures can take seconds on a large instance, so each panel that shows them
 * holds its own skeleton (or the failure) until they land.
 */
export function StorageView({
  title,
  subtitle,
}: {
  title: string;
  subtitle: string;
}) {
  const t = useTranslations('storage');
  const ts = useTranslations('settings');
  const queryClient = useQueryClient();
  // Read again when the saved status says to: a save leaves the next run for
  // the scheduler's next tick, and a scheduled cleanup runs at its time.
  const { data: retention } = useSuspenseQuery({
    ...storageQueries.retention(),
    refetchInterval: (query) =>
      query.state.data
        ? (nextReadIn(query.state.data, Date.now()) ?? false)
        : false,
  });
  const summaryRead = useQuery(storageQueries.summary());
  const projectsRead = useQuery(storageQueries.projects());
  const ageRead = useQuery(storageQueries.age());
  const [editing, setEditing] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [project, setProject] = useState<{ id: number; name: string } | null>(
    null,
  );
  const cleanup = useCleanupJob();

  // A cleanup this tab did not start (the schedule's) just finished: the
  // figures below are from before it.
  const scheduledRunning = retention.last_run?.status === 'running';
  const wasRunning = useRef(scheduledRunning);
  useEffect(() => {
    if (wasRunning.current && !scheduledRunning) {
      void queryClient.invalidateQueries({ queryKey: storageQueries.all() });
    }
    wasRunning.current = scheduledRunning;
  }, [scheduledRunning, queryClient]);

  const projects = projectsRead.data ?? null;
  const { data: storage } = summaryRead;
  const { data: age } = ageRead;
  const summary = useMemo(
    () =>
      storage && projects && age
        ? summarize({
            projects,
            age,
            sourceMaps: {
              files: storage.source_maps.file_count,
              bytes: storage.source_maps.total_bytes,
            },
            settings: retention.settings,
          })
        : null,
    [storage, projects, age, retention.settings],
  );
  // The figures are priced together, so one failed read fails them all; data
  // already on screen wins over a later failure.
  const failure = summary
    ? null
    : (summaryRead.error ?? projectsRead.error ?? ageRead.error);
  const saved = (job: CleanupStatus | null) => {
    if (job) cleanup.follow(job);
  };

  return (
    <div className="min-w-0">
      <SettingsHeader
        title={title}
        description={subtitle}
        actions={
          // Usable at once: the figures only price what the dialogs delete,
          // and the price fills in when they land.
          <>
            <Button
              variant="danger"
              icon={DeleteIcon}
              onClick={() => setDeleting(true)}
              disabled={cleanup.running}
            >
              {t('deleteAction')}
            </Button>
            <Button
              variant="primary"
              icon={ConfigureIcon}
              onClick={() => setEditing(true)}
            >
              {t('action')}
            </Button>
          </>
        }
      />

      {failure ? (
        <LoadFailure
          error={failure.error}
          title={ts('storage.loadFailed')}
          notFoundOnMissing={false}
        />
      ) : (
        <SettingSection title={t('overview.title')}>
          {summary ? (
            <StorageOverview summary={summary} />
          ) : (
            <OverviewSkeleton />
          )}
        </SettingSection>
      )}

      <SettingSection title={t('projects.retention')}>
        {failure ? null : summary ? (
          <CleanupLine
            summary={summary}
            retention={retention}
            job={cleanup.job}
          />
        ) : (
          <LineSkeleton />
        )}
        <CompactionNotice
          disabled={cleanup.running}
          onStarted={cleanup.follow}
        />
      </SettingSection>

      <SettingSection title={t('age.title')} description={t('age.description')}>
        <div className="px-5 py-4">
          {summary ? (
            <AgeChart buckets={summary.buckets} kinds={KINDS} />
          ) : (
            <ChartSkeleton />
          )}
        </div>
      </SettingSection>

      {summary ? (
        <StorageProjects
          projects={summary.projects}
          settings={retention.settings}
          onEditRetention={setProject}
        />
      ) : (
        <SettingSection title={t('projects.title')}>
          <ProjectsSkeleton />
        </SettingSection>
      )}

      <RetentionDialog
        open={editing}
        onOpenChange={setEditing}
        retention={retention}
        projects={projects}
        onDone={saved}
      />
      <DeleteDialog
        open={deleting}
        onOpenChange={setDeleting}
        projects={projects}
        onStarted={cleanup.follow}
      />
      <ProjectRetentionDialog
        project={project}
        onClose={() => setProject(null)}
        retention={retention}
        projects={projects}
        onDone={saved}
      />
    </div>
  );
}
