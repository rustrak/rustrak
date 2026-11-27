import type { LogResponse, LogsListResponse } from '@rustrak/client';
import {
  BugIcon,
  Button,
  ChevronLeftIcon,
  ChevronRightIcon,
  cn,
  DotIcon,
  ErrorIcon,
  focusRingInset,
  type IconComponent,
  InfoIcon,
  Text,
  WarningIcon,
} from '@rustrak/ui';
import { type ReactNode, useEffect, useRef } from 'react';
import { useFormatter, useTranslations } from 'use-intl';
import { matchParts } from '@/features/log/model/log-view';
import { levelTone } from '@/shared/config/levels';
import { useRelativeTime } from '@/shared/i18n/use-now';

/** The columns the header and every row share. */
const COLUMNS =
  'grid grid-cols-[5.5rem_minmax(0,1fr)_auto] items-center gap-4 sm:grid-cols-[11rem_5.5rem_minmax(0,1fr)_auto] md:grid-cols-[11rem_5.5rem_minmax(0,1fr)_8rem_auto]';

const GLYPH: Record<string, IconComponent> = {
  fatal: ErrorIcon,
  error: ErrorIcon,
  warn: WarningIcon,
  info: InfoIcon,
  debug: BugIcon,
  trace: DotIcon,
};

/**
 * The log lines, newest first, as a table: when, how bad, what, and where
 * from. Colour is kept to the level's icon, so an error is found at a glance
 * and nothing else shouts. A click opens the line in place, under its row,
 * and keeps your place in the list.
 */
export function LogList({
  logs,
  search,
  selectedId,
  detail,
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
  /** The open line in full, drawn under its row. */
  detail: ReactNode;
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
        <>
          <div
            aria-hidden="true"
            className={cn(
              COLUMNS,
              // The same head every DataTable has.
              'h-row-head shrink-0 border-border-subtle border-b bg-surface px-4 text-column text-fg-meta',
            )}
          >
            <span className="max-sm:hidden">{t('time')}</span>
            <span>{t('level')}</span>
            <span>{t('message')}</span>
            <span className="max-md:hidden">{t('environment')}</span>
            <span className="w-4" />
          </div>
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
              >
                {log.id === selectedId ? detail : null}
              </LogLine>
            ))}
          </ul>
        </>
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
  children,
}: {
  log: LogResponse;
  search: string;
  selected: boolean;
  onSelect: () => void;
  children: ReactNode;
}) {
  const t = useTranslations('logs');
  const ref = useRef<HTMLButtonElement>(null);
  // j and k move the selection; the line they land on is kept in view.
  useEffect(() => {
    if (selected) ref.current?.scrollIntoView({ block: 'nearest' });
  }, [selected]);

  return (
    <li>
      <button
        ref={ref}
        type="button"
        aria-expanded={selected}
        onClick={onSelect}
        className={cn(
          COLUMNS,
          'h-10 w-full px-4 text-left transition-colors',
          focusRingInset,
          selected ? 'bg-surface-selected' : 'hover:bg-surface-hover',
        )}
      >
        <LineTime timestamp={log.timestamp} />
        <LevelBadge level={log.level} />
        <span
          className={cn(
            'truncate font-mono text-mono-sm',
            levelTone(log.level) ? 'text-fg' : 'text-fg-muted',
          )}
        >
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
        <span className="truncate text-fg-subtle text-meta max-md:hidden">
          {log.environment ?? '—'}
        </span>
        <ChevronRightIcon
          size="sm"
          className={cn(
            'text-fg-ghost transition-transform duration-200',
            selected && 'rotate-90',
          )}
        />
      </button>
      {children}
    </li>
  );
}

const BADGE = {
  error: 'bg-sev-error-surface text-sev-error-fg',
  warning: 'bg-sev-warning-surface text-sev-warning-fg',
  info: 'bg-surface-chip text-fg-secondary',
  quiet: 'text-fg-ghost',
};

/**
 * The level as a small tinted label with its icon: errors and warnings carry
 * colour, info a neutral chip, debug and trace none, so the lines that matter
 * stand out of a burst without tinting whole rows.
 */
function LevelBadge({ level }: { level: string }) {
  const Glyph = GLYPH[level] ?? DotIcon;
  return (
    <span
      className={cn(
        'inline-flex h-5 w-fit items-center gap-1 rounded-sm px-1.5 font-medium font-mono text-badge uppercase tracking-wide',
        BADGE[levelTone(level) ?? 'quiet'],
      )}
    >
      <Glyph size="xs" className="shrink-0" />
      {level}
    </span>
  );
}

/**
 * The clock to the millisecond, the seconds bright and the rest receding,
 * so a burst of lines reads by its seconds.
 */
function LineTime({ timestamp }: { timestamp: string }) {
  const format = useFormatter();
  const relativeTime = useRelativeTime();
  const at = new Date(timestamp);
  const text = format.dateTime(at, 'logLine');
  const clock = text.match(/(\d{1,2}:\d{2}:\d{2})([.,]\d{3})/);
  return (
    <time
      dateTime={timestamp}
      title={relativeTime(at)}
      className="truncate font-mono text-fg-ghost text-mono-sm tabular-nums max-sm:hidden"
    >
      {clock?.index === undefined ? (
        text
      ) : (
        <>
          {text.slice(0, clock.index)}
          <span className="text-fg-secondary">{clock[1]}</span>
          {text.slice(clock.index + clock[1].length)}
        </>
      )}
    </time>
  );
}
