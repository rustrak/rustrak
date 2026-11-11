import type { ProjectResponse } from '@rustrak/client';
import {
  ChevronRightIcon,
  cn,
  DeleteIcon,
  focusRing,
  interactiveTransition,
  Menu,
  MoreIcon,
  Sparkline,
  Text,
} from '@rustrak/ui';
import { PlatformIcon } from 'platformicons';
import type { ReactNode } from 'react';
import { useFormatter, useTranslations } from 'use-intl';
import {
  type ProjectHealth,
  projectHealth,
} from '@/features/project/lib/health';
import { Link } from '@/shared/ui/components/link';
import { MetricDeltaText } from '@/shared/ui/components/metric-delta';

/**
 * Floor for the activity strip: it counts issues, and three concurrent issues
 * is already a busy hour for one project.
 */
const TREND_MIN_SCALE = 4;

interface ProjectCardProps {
  project: ProjectResponse;
  /** Absent when the reader may not delete it: no menu then. */
  onDelete?: (project: ProjectResponse) => void;
}

/**
 * One project on the projects page: who it is, whether it needs looking at,
 * and the four figures that decide which one you open first.
 *
 * Everything comes from the list response (`stats_period=24h`): a page of
 * cards is one request and two server queries, never one request per card.
 * The activity strip is the hand-drawn `Sparkline`, not a chart library, for
 * the same reason -- twenty cards must cost twenty SVGs and nothing else.
 *
 * The whole card is the link (the name's `::after` covers it), so the target
 * is the size of the card while the markup keeps one link with one name. The
 * actions menu sits above that layer.
 */
export function ProjectCard({ project, onDelete }: ProjectCardProps) {
  const t = useTranslations('projects');
  const format = useFormatter();
  const { stats } = project;
  const health = stats ? projectHealth(stats) : 'quiet';
  // `digested`, not `stored`: retention empties the store of a project that
  // has long been sending events.
  const waiting = project.digested_event_count === 0;

  return (
    <article
      className={cn(
        'group/card relative flex flex-col rounded-2xl border border-border-subtle',
        'bg-surface px-5 pt-4.5 pb-4',
        'hover:border-border-strong',
        interactiveTransition,
        // The ring belongs to the card, not to the name inside it: keyboard
        // focus has to show the same target a pointer would hit.
        'has-[a:focus-visible]:ring-2 has-[a:focus-visible]:ring-ring',
      )}
    >
      <div className="flex items-center gap-3">
        <PlatformIcon
          platform={project.platform ?? 'other'}
          size={34}
          format="lg"
          className="shrink-0 rounded-lg"
        />
        <div className="flex min-w-24 flex-1 flex-col">
          <Link
            href={`/projects/${project.id}`}
            className={cn(
              'truncate text-section text-fg outline-none',
              'after:absolute after:inset-0 after:rounded-2xl',
            )}
          >
            {project.name}
          </Link>
          <Text variant="mono-sm" tone="meta" truncate>
            {project.slug}
          </Text>
        </div>

        <HealthBadge
          health={waiting ? 'waiting' : health}
          fatal={stats?.fatal_issues ?? 0}
        />

        {onDelete ? (
          <Menu
            align="end"
            trigger={
              <button
                type="button"
                aria-label={t('card.actions', { name: project.name })}
                className={cn(
                  'relative z-10 flex size-control-xs items-center justify-center',
                  'rounded-md text-fg-meta hover:bg-surface-hover hover:text-fg',
                  'data-popup-open:bg-surface-hover',
                  interactiveTransition,
                  focusRing,
                )}
              >
                <MoreIcon size="lg" />
              </button>
            }
            actions={[
              {
                id: 'delete',
                label: t('actions.delete'),
                icon: DeleteIcon,
                tone: 'danger',
                onSelect: () => onDelete(project),
              },
            ]}
          />
        ) : null}

        <ChevronRightIcon
          size="lg"
          aria-hidden="true"
          className={cn(
            'shrink-0 text-fg-ghost',
            'transition-[translate,color] duration-fast ease-standard',
            'group-hover/card:translate-x-0.5 group-hover/card:text-fg-subtle',
            'motion-reduce:transition-none',
          )}
        />
      </div>

      <div className="mt-5">
        <div className="mb-2 flex items-baseline justify-between">
          <Text variant="hint" tone="meta">
            {t('columns.issueActivity')}
          </Text>
          <Text variant="hint" tone="ghost">
            {t('card.lastDay')}
          </Text>
        </div>
        {stats ? (
          <Sparkline
            values={stats.trend}
            tone={health === 'critical' ? 'danger' : 'neutral'}
            label={t('trendLabel', { label: t('columns.issueActivity') })}
            minScale={TREND_MIN_SCALE}
            className="h-10 w-full"
          />
        ) : (
          <div className="h-10" />
        )}
      </div>

      <dl className="mt-4 grid grid-cols-4 gap-3 border-t border-border-divider pt-3.5">
        <Figure
          label={t('columns.events24h')}
          value={stats ? format.number(stats.events.current, 'compact') : '–'}
          delta={stats ? <MetricDeltaText metric={stats.events} /> : null}
        />
        <Figure
          label={t('card.newIssues')}
          value={stats ? format.number(stats.new_issues.current) : '–'}
          delta={stats ? <MetricDeltaText metric={stats.new_issues} /> : null}
        />
        <Figure
          label={t('card.openIssues')}
          value={stats ? format.number(stats.open_issues) : '–'}
        />
        <Figure
          label={t('card.stored')}
          value={format.number(project.stored_event_count, 'compact')}
        />
      </dl>
    </article>
  );
}

function Figure({
  label,
  value,
  delta,
}: {
  label: string;
  value: string;
  delta?: ReactNode;
}) {
  return (
    <div className="flex min-w-0 flex-col gap-0.5">
      <dt className="truncate text-hint text-fg-meta">{label}</dt>
      <dd className="flex items-baseline gap-1.5">
        <span className="text-value font-semibold text-fg tabular-nums">
          {value}
        </span>
        {delta}
      </dd>
    </div>
  );
}

/**
 * The one line that says this project needs you. Quiet projects get none: a
 * badge on every card is a badge on no card.
 */
function HealthBadge({
  health,
  fatal,
}: {
  health: ProjectHealth | 'waiting';
  fatal: number;
}) {
  const t = useTranslations('projects');

  if (health === 'quiet') return null;

  const tone = {
    critical: 'bg-sev-error-surface text-sev-error-fg',
    rising: 'bg-sev-warning-surface text-sev-warning-fg',
    waiting: 'bg-surface-hover text-fg-subtle',
  }[health];

  const label = {
    critical: t('fatalCount', { count: fatal }),
    rising: t('card.rising'),
    waiting: t('card.waiting'),
  }[health];

  return (
    <span
      className={cn(
        // Shrinks before the name does: on a phone the name is what matters.
        'flex h-6 min-w-0 items-center gap-1.5 rounded-md px-2 text-meta font-medium',
        tone,
      )}
    >
      <span
        aria-hidden="true"
        className="size-1.5 shrink-0 rounded-pill bg-current"
      />
      <span className="truncate">{label}</span>
    </span>
  );
}
