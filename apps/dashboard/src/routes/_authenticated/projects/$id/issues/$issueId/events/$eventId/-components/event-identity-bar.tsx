import type { EventDetail } from '@rustrak/client';
import { useFormatter, useTranslations } from 'use-intl';
import type { EventNavigation } from '@/features/event/api/queries';
import type { EventJumpTarget } from '@/features/event/lib/event-jumps';
import { EventNavigationBar } from '@/features/event/ui/components/event-navigation';

interface EventIdentityBarProps {
  projectId: number;
  issueId: string;
  event: EventDetail;
  navigation: EventNavigation;
  jumps: { id: EventJumpTarget; label: string }[];
  outsideSelectedEnvironment: boolean;
}

/**
 * Which event this is, and where else to go in it.
 *
 * Sticks to the top on scroll from `sm` up: a stack trace is long, and losing
 * track of which of an issue's events is on screen while reading one is the
 * mistake this band exists to prevent.
 */
export function EventIdentityBar({
  projectId,
  issueId,
  event,
  navigation,
  jumps,
  outsideSelectedEnvironment,
}: EventIdentityBarProps) {
  const t = useTranslations('projectPages');
  const format = useFormatter();

  return (
    <div className="sm:sticky sm:top-0 sm:z-20 -mx-4 md:-mx-8 border-b bg-background px-4 md:px-8 pt-1 pb-3 space-y-3 sm:space-y-2">
      <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <p className="text-sm">
          <span className="font-semibold">{t('event.events')}</span>{' '}
          <span className="text-muted-foreground">
            {t('event.inThisIssue')}
          </span>
        </p>
        {outsideSelectedEnvironment ? (
          <span className="text-xs text-muted-foreground">
            {t('event.outsideSelectedEnvironment')}
          </span>
        ) : (
          <EventNavigationBar
            projectId={projectId}
            issueId={issueId}
            navigation={navigation}
          />
        )}
      </div>

      <div className="flex flex-col gap-3 text-xs text-muted-foreground sm:flex-row sm:items-center sm:justify-between sm:gap-2">
        <div className="flex items-center gap-2 flex-wrap min-w-0">
          <span>
            {t('event.idLabel')}{' '}
            <span className="font-mono text-foreground">
              {event.event_id.slice(0, 8)}
            </span>
          </span>
          <span className="text-muted-foreground/40">·</span>
          <span>{format.relativeTime(new Date(event.timestamp))}</span>
          {event.platform && (
            <span className="inline-flex items-center rounded-full bg-muted px-2 py-0.5 text-[11px] font-medium text-foreground/80">
              {event.platform}
            </span>
          )}
          {event.environment && (
            <span className="inline-flex items-center rounded-full bg-muted px-2 py-0.5 text-[11px] font-medium text-foreground/80">
              {event.environment}
            </span>
          )}
        </div>
        {jumps.length > 0 && (
          <div className="flex items-center gap-3 flex-wrap shrink-0">
            <span>{t('event.jumpTo')}</span>
            {jumps.map((j) => (
              <a
                key={j.id}
                href={`#${j.id}`}
                className="hover:text-foreground transition-colors"
              >
                {j.label}
              </a>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
