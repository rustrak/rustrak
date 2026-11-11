import type { EventDetailResponse } from '@rustrak/client';
import {
  Button,
  ChevronLeftIcon,
  ChevronRightIcon,
  CopyIcon,
  cn,
  Tag,
  Text,
  Tooltip,
} from '@rustrak/ui';
import { useEffect, useRef, useState } from 'react';
import { useFormatter, useTranslations } from 'use-intl';
import type { EventNavigation } from '@/features/event/api/queries';
import {
  type EventJumpTarget,
  sectionInView,
} from '@/features/event/lib/event-jumps';
import { useCopy } from '@/shared/ui/hooks/use-copy';
import { useRouter } from '@/shared/ui/hooks/use-router';
import { useShortcuts } from '@/shared/ui/hooks/use-shortcuts';

/**
 * Which of the issue's events is on screen, how to step to another, and
 * where to go inside this one.
 *
 * It sticks to the top of the column: a stack trace is long, and losing
 * track of which event is being read is the mistake this bar exists to
 * prevent. j and k step through events, [ and ] jump to the oldest and the
 * latest, the way they do in Sentry.
 */
export function EventNavigator({
  projectId,
  issueId,
  event,
  navigation,
  jumps,
}: {
  projectId: number;
  issueId: string;
  event: EventDetailResponse;
  navigation: EventNavigation;
  jumps: { id: EventJumpTarget; label: string }[];
}) {
  const t = useTranslations('issues');
  const { copy } = useCopy();
  const tEvents = useTranslations('events');
  const format = useFormatter();
  const router = useRouter();
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
      router.push(`/projects/${projectId}/issues/${issueId}/events/${eventId}`);
    }
  };

  const barRef = useRef<HTMLDivElement>(null);
  const [active, setActive] = useState<EventJumpTarget | undefined>(
    jumps[0]?.id,
  );

  /*
   * A click scrolls smoothly past every section between here and there, and
   * following the scroll would light each one on the way. So the clicked tab
   * holds until the scroll comes to rest -- no scroll event for a moment --
   * and only then does the bar go back to following it.
   */
  const pinned = useRef(false);
  const settle = useRef<ReturnType<typeof setTimeout>>(undefined);

  // Which section is under the bar, re-read on every scroll of whatever
  // scrolls -- the reading column from lg, the whole page below it.
  useEffect(() => {
    const read = (scroller: Element | null) => {
      const bar = barRef.current;
      if (!bar) return;
      // A jumped-to section lands `scroll-mt-32` below the column's top,
      // which is a little under the bar; the line sits at that landing spot
      // so a jump and the tab it lights agree.
      const line = bar.getBoundingClientRect().bottom + 48;
      const tops = jumps.map((jump) => ({
        id: jump.id,
        top:
          document.getElementById(jump.id)?.getBoundingClientRect().top ??
          Number.POSITIVE_INFINITY,
      }));
      const atEnd =
        !!scroller &&
        scroller.scrollTop > 0 &&
        scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 2;
      setActive(sectionInView(tops, line, atEnd));
    };
    const onScroll = (event: Event) => {
      // Only the scroller the bar lives in: the reading column, or the page
      // itself (whose scroll event targets the document). The rail, a JSON
      // block or any other nested scroller reaching its end says nothing
      // about where the reading is.
      const scroller =
        event.target instanceof Element
          ? event.target
          : document.scrollingElement;
      if (!scroller?.contains(barRef.current)) return;
      if (pinned.current) {
        clearTimeout(settle.current);
        settle.current = setTimeout(() => {
          pinned.current = false;
        }, 150);
        return;
      }
      read(scroller);
    };

    document.addEventListener('scroll', onScroll, true);
    return () => {
      document.removeEventListener('scroll', onScroll, true);
      clearTimeout(settle.current);
    };
  }, [jumps]);

  useShortcuts({
    j: () => go(nextEventId),
    k: () => go(prevEventId),
    '[': () => go(firstEventId),
    ']': () => go(lastEventId),
  });

  const copyId = () => copy(event.event_id, t('table.copied'));

  return (
    <div
      ref={barRef}
      className="sticky top-0 z-20 -mx-page-gutter border-border-divider border-b bg-panel/95 px-page-gutter pt-3 backdrop-blur"
    >
      <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2 pb-1">
        {/* Two joined groups, each read as one control: step to the
            neighbouring event, or jump to either end. The keys live in the
            tooltips, not in the labels. */}
        <div className="flex flex-wrap items-center gap-3">
          <div className="inline-flex items-center">
            <Tooltip content={`${tEvents('navigation.previousEvent')} · K`}>
              <Button
                variant="secondary"
                size="sm"
                icon={ChevronLeftIcon}
                aria-label={tEvents('navigation.previousEvent')}
                disabled={!prevEventId}
                onClick={() => go(prevEventId)}
                className="rounded-e-none"
              />
            </Tooltip>
            <Tooltip content={`${tEvents('navigation.nextEvent')} · J`}>
              <Button
                variant="secondary"
                size="sm"
                icon={ChevronRightIcon}
                aria-label={tEvents('navigation.nextEvent')}
                disabled={!nextEventId}
                onClick={() => go(nextEventId)}
                className="-ms-px rounded-s-none"
              />
            </Tooltip>
          </div>

          <Text variant="value" className="tabular-nums text-fg">
            {t('detail.eventPosition', {
              current: format.number(currentIndex),
              total: format.number(totalCount),
            })}
          </Text>

          <div className="inline-flex items-center">
            <Tooltip content={`${tEvents('navigation.first')} · [`}>
              <Button
                variant="secondary"
                size="sm"
                disabled={currentIndex <= 1 || !firstEventId}
                onClick={() => go(firstEventId)}
                className="rounded-e-none"
              >
                {tEvents('navigation.first')}
              </Button>
            </Tooltip>
            <Tooltip content={`${tEvents('navigation.latest')} · ]`}>
              <Button
                variant="secondary"
                size="sm"
                disabled={currentIndex >= totalCount || !lastEventId}
                onClick={() => go(lastEventId)}
                className="-ms-px rounded-s-none"
              >
                {tEvents('navigation.latest')}
              </Button>
            </Tooltip>
          </div>
        </div>

        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <button
            type="button"
            onClick={copyId}
            title={t('detail.copyEventId')}
            className="group/id inline-flex items-center gap-1.5 font-mono text-mono-sm text-fg-subtle hover:text-fg"
          >
            {event.event_id.slice(0, 8)}
            <CopyIcon
              size="sm"
              aria-hidden="true"
              className="opacity-0 group-hover/id:opacity-100"
            />
          </button>
          <Text
            variant="meta"
            tone="subtle"
            title={format.dateTime(new Date(event.timestamp), 'dateTime')}
          >
            {format.relativeTime(new Date(event.timestamp))}
          </Text>
          {event.environment ? (
            <Tag variant="soft" verbatim>
              {event.environment}
            </Tag>
          ) : null}
          {event.release ? (
            <Tag variant="soft" verbatim className="font-mono">
              {event.release}
            </Tag>
          ) : null}
          {event.platform ? (
            <Tag variant="soft" verbatim>
              {event.platform}
            </Tag>
          ) : null}
        </div>
      </div>

      {jumps.length > 0 ? (
        // In-page tabs: one per section of this event, the one being read
        // underlined. They follow the scroll, so the bar always says where
        // in the event you are, and a click moves there.
        <nav
          aria-label={t('detail.jumpTo')}
          className="-mb-px mt-1 flex gap-5 overflow-x-auto"
        >
          {jumps.map((jump) => (
            <a
              key={jump.id}
              href={`#${jump.id}`}
              aria-current={active === jump.id ? 'location' : undefined}
              onClick={(event) => {
                event.preventDefault();
                setActive(jump.id);
                pinned.current = true;
                // A section already in place scrolls nowhere and fires
                // nothing; this lets go of the pin in that case too.
                clearTimeout(settle.current);
                settle.current = setTimeout(() => {
                  pinned.current = false;
                }, 600);
                document
                  .getElementById(jump.id)
                  ?.scrollIntoView({ behavior: 'smooth', block: 'start' });
              }}
              className={cn(
                'relative flex h-9 shrink-0 items-center whitespace-nowrap text-meta',
                'text-fg-subtle hover:text-fg-secondary',
                "after:absolute after:inset-x-0 after:bottom-0 after:h-0.5 after:rounded-pill after:content-['']",
                'aria-[current=location]:font-medium aria-[current=location]:text-fg',
                'aria-[current=location]:after:bg-surface-brand',
              )}
            >
              {jump.label}
            </a>
          ))}
        </nav>
      ) : null}
    </div>
  );
}
