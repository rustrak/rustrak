import type { LogResponse, LogsListResponse } from '@rustrak/client';
import {
  Button,
  ChevronLeftIcon,
  ChevronRightIcon,
  cn,
  focusRingInset,
  Tag,
  Text,
} from '@rustrak/ui';
import { type ReactNode, useEffect, useRef } from 'react';
import { useFormatter, useTranslations } from 'use-intl';
import { matchParts } from '@/features/log/model/log-view';
import { levelMark, levelTone } from '@/shared/config/levels';
import { useRelativeTime } from '@/shared/i18n/use-now';

/**
 * The log lines, newest first, one line each: a stripe and a word for the
 * level, the time to the millisecond, and the message in monospace with
 * what the search matched marked. A click opens the line beside the list.
 */
export function LogList({
  logs,
  search,
  selectedId,
  page,
  isPending,
  onSelect,
  onPageChange,
  emptyHint,
}: {
  logs: LogsListResponse;
  /** The free text the list was searched with, marked in each message. */
  search: string;
  selectedId: string | undefined;
  page: number;
  isPending: boolean;
  onSelect: (id: string) => void;
  onPageChange: (page: number) => void;
  /** Said under an empty list, e.g. how to send logs when nothing narrows it. */
  emptyHint?: ReactNode;
}) {
  const t = useTranslations('logs');

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {logs.items.length === 0 ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-2 px-4 py-16 text-center">
          <Text variant="meta" tone="subtle">
            {t('empty.title')}
          </Text>
          {emptyHint ? (
            <Text variant="meta" tone="subtle" className="max-w-md">
              {emptyHint}
            </Text>
          ) : null}
        </div>
      ) : (
        <ul
          aria-label={t('lines')}
          className={cn(
            'min-h-0 flex-1 divide-y divide-border-divider overflow-y-auto',
            isPending && 'opacity-60',
          )}
        >
          {logs.items.map((log) => (
            <LogLine
              key={log.id}
              log={log}
              search={search}
              selected={log.id === selectedId}
              onSelect={() => onSelect(log.id)}
            />
          ))}
        </ul>
      )}

      {logs.total_pages > 1 ? (
        <div className="flex shrink-0 items-center justify-between gap-2 border-border-divider border-t px-4 py-2">
          <Text variant="meta" tone="subtle">
            {t('pageOf', { page, total: logs.total_pages })}
          </Text>
          <div className="flex gap-1">
            <Button
              size="sm"
              variant="ghost"
              icon={ChevronLeftIcon}
              aria-label={t('newer')}
              disabled={isPending || page <= 1}
              onClick={() => onPageChange(page - 1)}
            />
            <Button
              size="sm"
              variant="ghost"
              icon={ChevronRightIcon}
              aria-label={t('older')}
              disabled={isPending || page >= logs.total_pages}
              onClick={() => onPageChange(page + 1)}
            />
          </div>
        </div>
      ) : null}
    </div>
  );
}

function LogLine({
  log,
  search,
  selected,
  onSelect,
}: {
  log: LogResponse;
  search: string;
  selected: boolean;
  onSelect: () => void;
}) {
  const t = useTranslations('logs');
  const format = useFormatter();
  const relativeTime = useRelativeTime();
  const ref = useRef<HTMLButtonElement>(null);
  const at = new Date(log.timestamp);

  // j and k move the selection; the line they land on is kept in view.
  useEffect(() => {
    if (selected) ref.current?.scrollIntoView({ block: 'nearest' });
  }, [selected]);

  return (
    <li>
      <button
        ref={ref}
        type="button"
        aria-current={selected || undefined}
        onClick={onSelect}
        className={cn(
          'flex w-full items-center gap-3 py-1.5 pr-4 pl-2 text-left font-mono text-mono-sm transition-colors',
          focusRingInset,
          selected ? 'bg-surface-selected' : 'hover:bg-surface-hover',
        )}
      >
        <span
          aria-hidden="true"
          className={cn(
            'h-4 w-0.75 shrink-0 rounded-pill',
            levelMark(log.level, 'bg-border-control'),
          )}
        />
        <time
          dateTime={log.timestamp}
          title={relativeTime(at)}
          className="hidden shrink-0 text-fg-subtle tabular-nums sm:block"
        >
          {format.dateTime(at, 'logLine')}
        </time>
        <Tag tone={levelTone(log.level) ?? 'neutral'} className="w-10 shrink-0">
          {log.level}
        </Tag>
        <span className="min-w-0 flex-1 truncate text-fg">
          {log.body ? (
            matchParts(log.body, search).map((part, index) =>
              part.match ? (
                // Parts are positional by nature: index is their identity.
                <mark
                  key={index}
                  className="rounded-xs bg-sev-warning-surface text-fg"
                >
                  {part.text}
                </mark>
              ) : (
                part.text
              ),
            )
          ) : (
            <span className="text-fg-ghost">{t('emptyValue')}</span>
          )}
        </span>
      </button>
    </li>
  );
}
