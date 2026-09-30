import { useTransition } from 'react';
import { useTranslations } from 'use-intl';
import {
  AGENT_PERIODS,
  agentDashboardQuery,
} from '@/features/agent-trace/model/filters';
import { Button } from '@/shared/ui/components/shadcn/button';
import { useRouter } from '@/shared/ui/hooks/use-router';

interface AgentDashboardFiltersProps {
  projectId: number;
  /** Raw search params, so the query builder owns every rewrite. */
  current: { period?: string; environment?: string };
}

/**
 * Time-window selector. The shared project layout owns the environment selector.
 */
export function AgentDashboardFilters({
  projectId,
  current,
}: AgentDashboardFiltersProps) {
  const router = useRouter();
  const t = useTranslations('agents.filters');
  const [isPending, startTransition] = useTransition();

  const navigate = (change: {
    period?: string | null;
    environment?: string | null;
  }) => {
    startTransition(() => {
      router.push(
        `/projects/${projectId}/agents${agentDashboardQuery(current, change)}`,
      );
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
            onClick={() => navigate({ period })}
            disabled={isPending}
          >
            {period}
          </Button>
        ))}
        <Button
          variant={!current.period ? 'secondary' : 'ghost'}
          size="sm"
          className="h-7 px-3"
          onClick={() => navigate({ period: null })}
          disabled={isPending}
        >
          {t('allTime')}
        </Button>
      </div>
    </div>
  );
}
