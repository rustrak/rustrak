import { ReleaseIcon } from '@rustrak/ui';
import { useQuery } from '@tanstack/react-query';
import { useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import type { PeriodChoice } from '@/shared/lib/period';
import { scopeActions } from '@/shared/lib/scope-actions';
import {
  EnvironmentSegment,
  PeriodSegment,
} from '@/shared/ui/components/period-filter';
import { ScopeBar, ScopeSegment } from '@/shared/ui/components/scope-bar';

interface FilterChange {
  period?: PeriodChoice;
  environment?: string;
  release?: string;
}

interface OverviewFiltersProps {
  projectId: number;
  period: PeriodChoice;
  environment?: string;
  release?: string;
  /** In the skeleton: its choices are not asked for, see `heldReads`. */
  held?: boolean;
  onChange: (change: FilterChange) => void;
}

/**
 * The page's filters, in its header: they scope every panel below at once,
 * which is what sets them apart from a table's own filters, that live with
 * the table.
 *
 * One joined bar rather than three buttons, so it reads as one control that
 * says what slice of the project you are looking at. Weight follows meaning:
 * a scope that is not set shows only its name, quietly; one that is set
 * shows its value, bright, with the brand dot. Held in the URL, so a filtered
 * view survives a reload and a shared link.
 */
export function OverviewFilters({
  projectId,
  period,
  environment,
  release,
  held = false,
  onChange,
}: OverviewFiltersProps) {
  const t = useTranslations('projectPages.overview');
  // A filter's choices are empty while they load or when they fail to: the
  // filter still works, it just has nothing to offer yet.
  const idle = { enabled: !held, subscribed: !held };
  const environments =
    useQuery({ ...projectQueries.environments(projectId), ...idle }).data ?? [];
  const releases =
    useQuery({ ...projectQueries.releaseVersions(projectId), ...idle }).data ??
    [];

  return (
    <ScopeBar>
      <EnvironmentSegment
        environments={environments}
        environment={environment}
        onChange={(value) => onChange({ environment: value })}
      />
      <ScopeSegment
        icon={ReleaseIcon}
        label={release ?? t('release')}
        active={release !== undefined}
        mono={release !== undefined}
        actions={scopeActions(
          releases,
          release,
          t('allReleases'),
          t('noReleases'),
          (value) => onChange({ release: value }),
        )}
      />
      <PeriodSegment
        period={period}
        onChange={(value) => onChange({ period: value })}
      />
    </ScopeBar>
  );
}
