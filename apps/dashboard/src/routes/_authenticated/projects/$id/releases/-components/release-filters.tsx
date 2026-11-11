import { useQuery } from '@tanstack/react-query';
import { projectQueries } from '@/features/project/api/queries';
import type { PeriodChoice } from '@/shared/lib/period';
import {
  EnvironmentSegment,
  PeriodSegment,
} from '@/shared/ui/components/period-filter';
import { ScopeBar } from '@/shared/ui/components/scope-bar';

export interface ReleaseScope {
  environment?: string;
  period: PeriodChoice;
}

/** What a release's page counts: one environment or all, over a window. */
export function ReleaseFilters({
  projectId,
  scope,
  held = false,
  onChange,
}: {
  projectId: number;
  scope: ReleaseScope;
  /** In the skeleton: the choices are not asked for. */
  held?: boolean;
  onChange: (change: Partial<ReleaseScope>) => void;
}) {
  const environments =
    useQuery({
      ...projectQueries.environments(projectId),
      enabled: !held,
      subscribed: !held,
    }).data ?? [];

  return (
    <ScopeBar>
      <EnvironmentSegment
        environments={environments}
        environment={scope.environment}
        onChange={(environment) => onChange({ environment })}
      />
      <PeriodSegment
        period={scope.period}
        onChange={(period) => onChange({ period })}
      />
    </ScopeBar>
  );
}
