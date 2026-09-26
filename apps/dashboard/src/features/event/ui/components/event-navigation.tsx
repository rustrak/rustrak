import { useNavigate } from '@tanstack/react-router';
import { ChevronLeft, ChevronRight } from 'lucide-react';
import { useTranslations } from 'use-intl';
import type { EventNavigation } from '@/features/event/api/queries';
import { Button } from '@/shared/ui/components/shadcn/button';

interface EventNavigationBarProps {
  projectId: number;
  issueId: string;
  navigation: EventNavigation;
}

export function EventNavigationBar({
  projectId,
  issueId,
  navigation,
}: EventNavigationBarProps) {
  const t = useTranslations('events');
  const navigate = useNavigate();
  const {
    currentIndex,
    totalCount,
    firstEventId,
    lastEventId,
    prevEventId,
    nextEventId,
  } = navigation;

  const go = (eventId?: string | null) => {
    if (eventId) {
      navigate({
        to: '/projects/$id/issues/$issueId/events/$eventId',
        params: { id: projectId, issueId, eventId },
      });
    }
  };

  return (
    <div className="flex items-center gap-1">
      <Button
        variant="ghost"
        size="sm"
        className="size-8 px-0"
        aria-label={t('navigation.previousEvent')}
        disabled={!prevEventId}
        onClick={() => go(prevEventId)}
      >
        <ChevronLeft className="size-4" />
      </Button>
      <Button
        variant="ghost"
        size="sm"
        className="size-8 px-0"
        aria-label={t('navigation.nextEvent')}
        disabled={!nextEventId}
        onClick={() => go(nextEventId)}
      >
        <ChevronRight className="size-4" />
      </Button>

      <span className="mx-1.5 text-xs tabular-nums text-muted-foreground">
        {t('navigation.position', {
          current: currentIndex,
          total: totalCount,
        })}
      </span>

      <Button
        variant="ghost"
        size="sm"
        disabled={currentIndex <= 1 || !firstEventId}
        onClick={() => go(firstEventId)}
      >
        {t('navigation.first')}
      </Button>
      <Button
        variant="ghost"
        size="sm"
        disabled={currentIndex >= totalCount || !lastEventId}
        onClick={() => go(lastEventId)}
      >
        {t('navigation.latest')}
      </Button>
    </div>
  );
}
