import type { CleanupStatus, RetentionResponse } from '@rustrak/client';
import { cn, SpinnerIcon } from '@rustrak/ui';
import { useFormatter, useTranslations } from 'use-intl';
import {
  deletesAnything,
  removedParts,
  type Summary,
} from '@/features/storage/model/retention';
import { useRelativeTime } from '@/shared/i18n/use-now';
import { formatBytes } from '@/shared/lib/utils';

/** Fixed categorical slots, in order: the colour follows the kind. */
const SEGMENTS = [
  { key: 'events', color: 'bg-chart-1' },
  { key: 'transactions', color: 'bg-chart-2' },
  { key: 'logs', color: 'bg-chart-3' },
  { key: 'sourceMaps', color: 'bg-chart-4' },
] as const;

/**
 * How much the instance holds and of what, as one meter. What the cleanup
 * does next is `CleanupLine`, in the retention section. Each figure appears
 * here and nowhere else.
 */
export function StorageOverview({ summary }: { summary: Summary }) {
  const t = useTranslations('storage');
  const format = useFormatter();
  const total = Math.max(summary.bytes, 1);
  const bytesOf = (key: (typeof SEGMENTS)[number]['key']) =>
    key === 'sourceMaps' ? summary.sourceMaps.bytes : summary.kinds[key].bytes;
  const detailOf = (key: (typeof SEGMENTS)[number]['key']) =>
    key === 'sourceMaps'
      ? t('overview.files', { count: summary.sourceMaps.files })
      : t('overview.rows', {
          count: summary.kinds[key].rows,
          rows: format.number(summary.kinds[key].rows, 'compact'),
        });

  return (
    <div className="space-y-4 px-5 py-4">
      <p className="text-numeric-lg tabular-nums text-fg">
        {formatBytes(summary.bytes)}
      </p>
      <div
        className="flex h-3 w-full gap-0.5 overflow-hidden rounded-full bg-surface-chip"
        role="img"
        aria-label={SEGMENTS.map(
          (s) => `${t(`kind.${s.key}`)} ${formatBytes(bytesOf(s.key))}`,
        ).join(', ')}
      >
        {SEGMENTS.map((s) =>
          bytesOf(s.key) > 0 ? (
            <div
              key={s.key}
              className={s.color}
              style={{ width: `${(bytesOf(s.key) / total) * 100}%` }}
            />
          ) : null,
        )}
      </div>
      <dl className="grid grid-cols-2 gap-4 lg:grid-cols-4">
        {SEGMENTS.map((s) => (
          <div key={s.key} className="flex gap-2">
            <span
              aria-hidden
              className={cn('mt-1.5 size-2.5 shrink-0 rounded-xs', s.color)}
            />
            <div>
              <dt className="text-meta text-fg-subtle">{t(`kind.${s.key}`)}</dt>
              <dd className="text-numeric tabular-nums text-fg">
                {formatBytes(bytesOf(s.key))}
              </dd>
              <dd className="text-hint text-fg-meta">{detailOf(s.key)}</dd>
            </div>
          </div>
        ))}
      </dl>
    </div>
  );
}

/** A row inside a `SettingSection`, the same padding and rule as `SettingRow`. */
const ROW = 'border-b border-border-divider px-5 py-4 last:border-b-0';

/** One sentence: what the cleanup does next, or is doing now. */
export function CleanupLine({
  summary,
  retention,
  job,
}: {
  summary: Summary;
  retention: RetentionResponse;
  job: CleanupStatus | null;
}) {
  const t = useTranslations('storage');
  const relativeTime = useRelativeTime();

  if (job?.state === 'running' && job.kind === 'compaction') {
    return (
      <p className={cn(ROW, 'flex items-center gap-2 text-value text-fg')}>
        <SpinnerIcon
          size="sm"
          aria-hidden="true"
          className="animate-spin motion-reduce:animate-none"
        />
        {t('overview.compacting')}
      </p>
    );
  }
  // Running here (followed, with progress) or on the schedule.
  if (job?.state === 'running' || retention.last_run?.status === 'running') {
    const parts = job?.state === 'running' ? removedParts(job.removed, t) : [];
    return (
      <p className={cn(ROW, 'flex items-center gap-2 text-value text-fg')}>
        <SpinnerIcon
          size="sm"
          aria-hidden="true"
          className="animate-spin motion-reduce:animate-none"
        />
        {t('overview.cleaning', {
          parts: parts.length ? parts.join(', ') : t('overview.starting'),
        })}
      </p>
    );
  }
  if (!deletesAnything(retention.settings)) {
    return (
      <p className={cn(ROW, 'text-value text-fg-subtle')}>
        {t('overview.off')}
      </p>
    );
  }
  const last = retention.last_run;
  return (
    <div className={cn(ROW, 'space-y-0.5 text-value text-fg')}>
      <p>
        {retention.next_run_at
          ? t('overview.next', {
              when: relativeTime(new Date(retention.next_run_at)),
              size: formatBytes(summary.frees),
            })
          : t('overview.scheduling')}
      </p>
      {last?.status === 'error' && (
        <p className="text-meta text-danger-fg">
          {t('status.lastRunFailed', {
            error: last.error ?? '',
            when: relativeTime(new Date(last.finished_at ?? last.started_at)),
          })}
        </p>
      )}
    </div>
  );
}
