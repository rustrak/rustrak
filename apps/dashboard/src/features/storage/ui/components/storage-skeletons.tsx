import { Skeleton } from '@rustrak/ui';
import { useTranslations } from 'use-intl';
import { SettingSection } from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';

/**
 * `StorageView` before its retention lands: the same header, without the
 * actions that need it, and every panel in its figures-loading state.
 */
export function StorageViewSkeleton({
  title,
  subtitle,
}: {
  title: string;
  subtitle: string;
}) {
  const t = useTranslations('storage');
  return (
    <div className="min-w-0">
      <SettingsHeader title={title} description={subtitle} />
      <SettingSection title={t('overview.title')}>
        <OverviewSkeleton />
      </SettingSection>
      <SettingSection title={t('projects.retention')}>
        <LineSkeleton />
      </SettingSection>
      <SettingSection title={t('age.title')} description={t('age.description')}>
        <div className="px-5 py-4">
          <ChartSkeleton />
        </div>
      </SettingSection>
      <SettingSection title={t('projects.title')}>
        <ProjectsSkeleton />
      </SettingSection>
    </div>
  );
}

/** The overview's place while the figures load, inside its section. */
export function OverviewSkeleton() {
  return (
    <div className="space-y-3 px-5 py-4" aria-hidden="true">
      <Skeleton className="h-7 w-28" />
      <Skeleton className="h-3 w-full rounded-full" />
      <div className="flex gap-5">
        {Array.from({ length: 4 }).map((_, i) => (
          <Skeleton key={i} className="h-4 w-28" />
        ))}
      </div>
    </div>
  );
}

/** The cleanup sentence's place while the figures load. */
export function LineSkeleton() {
  return (
    <div
      className="border-b border-border-divider px-5 py-4 last:border-b-0"
      aria-hidden="true"
    >
      <Skeleton className="h-4 w-64" />
    </div>
  );
}

/** The age chart's place while the figures load. */
export function ChartSkeleton() {
  return <Skeleton className="h-48 w-full" />;
}

/** The projects table's place while the figures load, inside its section. */
export function ProjectsSkeleton() {
  return (
    <div aria-hidden="true">
      {Array.from({ length: 4 }).map((_, i) => (
        <div
          key={i}
          className="flex items-center justify-between border-b border-border-divider px-5 py-4 last:border-b-0"
        >
          <Skeleton className="h-4 w-32" />
          <Skeleton className="h-4 w-48" />
        </div>
      ))}
    </div>
  );
}
