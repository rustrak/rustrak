import type { FeedbackListResponse, FeedbackResponse } from '@rustrak/client';
import {
  AttachmentIcon,
  Avatar,
  Button,
  ChevronLeftIcon,
  ChevronRightIcon,
  cn,
  ErrorIcon,
  focusRingInset,
  LinkIcon,
  Text,
} from '@rustrak/ui';
import { useEffect, useRef } from 'react';
import { useFormatter, useTranslations } from 'use-intl';
import {
  feedbackSender,
  shortUrl,
} from '@/features/issue/model/feedback-mailbox';

/**
 * The inbox's left column: who wrote, when, and the start of what they said,
 * newest first. One row is always the open one; the reading pane beside it
 * shows that row, the way a mail client does.
 */
export function FeedbackList({
  feedback,
  selectedId,
  page,
  isPending,
  projectNames,
  onSelect,
  onPageChange,
}: {
  feedback: FeedbackListResponse;
  selectedId: string | undefined;
  page: number;
  isPending: boolean;
  /** Across projects, each row names the project it came from. */
  projectNames?: ReadonlyMap<number, string>;
  onSelect: (id: string) => void;
  onPageChange: (page: number) => void;
}) {
  const t = useTranslations('projectPages.feedback');

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <ul
        aria-label={t('title')}
        className={cn(
          'min-h-0 flex-1 divide-y divide-border-divider overflow-y-auto',
          isPending && 'opacity-60',
        )}
      >
        {feedback.items.map((item) => (
          <FeedbackRow
            key={item.id}
            item={item}
            selected={item.id === selectedId}
            project={projectNames?.get(item.project_id)}
            onSelect={() => onSelect(item.id)}
          />
        ))}
      </ul>

      {feedback.total_pages > 1 ? (
        <div className="flex shrink-0 items-center justify-between gap-2 border-border-divider border-t px-4 py-2">
          <Text variant="meta" tone="subtle">
            {t('pageOf', { page, total: feedback.total_pages })}
          </Text>
          <div className="flex gap-1">
            <Button
              size="sm"
              variant="ghost"
              icon={ChevronLeftIcon}
              aria-label={t('previousPage')}
              disabled={isPending || page <= 1}
              onClick={() => onPageChange(page - 1)}
            />
            <Button
              size="sm"
              variant="ghost"
              icon={ChevronRightIcon}
              aria-label={t('nextPage')}
              disabled={isPending || page >= feedback.total_pages}
              onClick={() => onPageChange(page + 1)}
            />
          </div>
        </div>
      ) : null}
    </div>
  );
}

function FeedbackRow({
  item,
  selected,
  project,
  onSelect,
}: {
  item: FeedbackResponse;
  selected: boolean;
  project?: string;
  onSelect: () => void;
}) {
  const t = useTranslations('projectPages.feedback');
  const format = useFormatter();
  const ref = useRef<HTMLButtonElement>(null);
  const sender = feedbackSender(item) || t('anonymous');

  // j and k move the selection; the row they land on is kept in view.
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
          'flex w-full gap-3 px-4 py-3 text-left transition-colors',
          focusRingInset,
          // Narrow, the list is only on screen while nothing is open, so a
          // highlight there would point at a pane that is not shown.
          selected ? 'lg:bg-surface-selected' : 'hover:bg-surface-hover',
        )}
      >
        <Avatar name={sender} size="sm" className="mt-0.5" />
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <div className="flex items-baseline gap-2">
            <span className="min-w-0 flex-1 truncate font-medium text-body text-fg">
              {sender}
            </span>
            {project ? (
              <span className="max-w-32 shrink-0 truncate text-fg-subtle text-meta">
                {project}
              </span>
            ) : null}
            <time
              dateTime={item.created_at}
              className="shrink-0 text-fg-subtle text-meta tabular-nums"
            >
              {format.relativeTime(new Date(item.created_at))}
            </time>
          </div>
          <p className="line-clamp-2 break-words text-body text-fg-secondary">
            {item.message}
          </p>
          {item.url ||
          item.attachments.length > 0 ||
          item.associated_event_id ? (
            <div className="mt-1 flex min-w-0 items-center gap-3 text-fg-subtle text-meta">
              {item.url ? (
                <span className="flex min-w-0 items-center gap-1">
                  <LinkIcon size="sm" aria-hidden="true" />
                  <span className="truncate font-mono text-mono-sm">
                    {shortUrl(item.url)}
                  </span>
                </span>
              ) : null}
              {item.attachments.length > 0 ? (
                <span
                  className="flex shrink-0 items-center gap-1"
                  title={t('attachments', { count: item.attachments.length })}
                >
                  <AttachmentIcon size="sm" aria-hidden="true" />
                  {item.attachments.length}
                </span>
              ) : null}
              {item.associated_event_id ? (
                <span
                  className="flex shrink-0 items-center gap-1 text-sev-error-fg"
                  title={t('linkedError')}
                >
                  <ErrorIcon size="sm" aria-hidden="true" />
                </span>
              ) : null}
            </div>
          ) : null}
        </div>
      </button>
    </li>
  );
}
