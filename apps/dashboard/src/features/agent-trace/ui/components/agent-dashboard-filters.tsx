import { useNavigate } from '@tanstack/react-router';
import { useTransition } from 'react';
import { useTranslations } from 'use-intl';
import {
  AGENT_PERIODS,
  agentDashboardSearch,
} from '@/features/agent-trace/model/filters';
import { cn } from '@/shared/lib/utils';
import { Button } from '@/shared/ui/components/shadcn/button';

interface AgentDashboardFiltersProps {
  projectId: number;
  /** Raw search params, so the query builder owns every rewrite. */
  current: { period?: string; environment?: string };
  /** Read from the data — an installation names its environments freely. */
  environments: string[];
}

/**
 * Window and environment selectors, held in the URL so the dashboard stays a
 * Server Component and a filtered view survives a reload or a shared link.
 */
export function AgentDashboardFilters({
  projectId,
  current,
  environments,
}: AgentDashboardFiltersProps) {
  const navigate = useNavigate();
  const t = useTranslations('agents.filters');
  const [isPending, startTransition] = useTransition();

  const go = (change: {
    period?: string | null;
    environment?: string | null;
  }) => {
    startTransition(() => {
      navigate({
        to: '/projects/$id/agents',
        params: { id: projectId },
        search: agentDashboardSearch(current, change),
      });
    });
  };

  return (
    <div
      className="flex flex-wrap items-center gap-2"
      data-pending={isPending ? '' : undefined}
    >
      <div className="flex w-fit items-center gap-1 rounded-lg border bg-muted/30 p-1">
        {AGENT_PERIODS.map((period) => (
          <Button
            key={period}
            variant={current.period === period ? 'secondary' : 'ghost'}
            size="sm"
            className="h-7 px-3"
            onClick={() => go({ period })}
            disabled={isPending}
          >
            {period}
          </Button>
        ))}
        <Button
          variant={!current.period ? 'secondary' : 'ghost'}
          size="sm"
          className="h-7 px-3"
          onClick={() => go({ period: null })}
          disabled={isPending}
        >
          {t('allTime')}
        </Button>
      </div>

      {/* Only offered when there is a choice to make: a single-environment
          install gains nothing from a dropdown with one entry. */}
      {environments.length > 0 && (
        <div className="flex w-fit items-center gap-1 rounded-lg border bg-muted/30 p-1">
          <Button
            variant={!current.environment ? 'secondary' : 'ghost'}
            size="sm"
            className="h-7 px-3"
            onClick={() => go({ environment: null })}
            disabled={isPending}
          >
            {t('allEnvironments')}
          </Button>
          {environments.map((environment) => (
            <Button
              key={environment}
              variant={
                current.environment === environment ? 'secondary' : 'ghost'
              }
              size="sm"
              className={cn('h-7 px-3 font-mono')}
              onClick={() => go({ environment })}
              disabled={isPending}
            >
              {environment}
            </Button>
          ))}
        </div>
      )}
    </div>
  );
}
