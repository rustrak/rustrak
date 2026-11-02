import { Select, SelectItem, Tag } from '@rustrak/ui';
import { useMemo, useState } from 'react';
import { useTranslations } from 'use-intl';
import {
  type ExceptionChain,
  findBestThread,
  formatStackTraceAsText,
  matchExceptionForThread,
  orderFramesForDisplay,
  type Thread,
} from '@/features/event/lib/format-stack-trace';
import { CopyAsDropdown } from '@/shared/ui/components/copy-as-dropdown';
import { type FrameSourceUrl, StackFrameItem } from './stack-frame-item';

interface ThreadsSectionProps {
  threads: Thread[];
  exception?: ExceptionChain;
  platform?: string;
  sourceUrl?: FrameSourceUrl;
}

function threadLabel(
  thread: Thread,
  t: (key: string, values?: Record<string, string | number>) => string,
): string {
  return thread.name || t('threads.threadLabel', { id: thread.id ?? '?' });
}

function threadKey(thread: Thread, index: number): string {
  return thread.id !== undefined ? String(thread.id) : `idx-${index}`;
}

export function ThreadsSection({
  threads,
  exception,
  platform,
  sourceUrl,
}: ThreadsSectionProps) {
  const t = useTranslations('events');
  const bestThread = useMemo(() => findBestThread(threads), [threads]);
  const [activeKey, setActiveKey] = useState<string>(() =>
    bestThread ? threadKey(bestThread, threads.indexOf(bestThread)) : '',
  );

  if (threads.length === 0) {
    return (
      <div className="py-12 text-center text-body text-fg-muted">
        {t('stackTrace.empty')}
      </div>
    );
  }

  const activeThread =
    threads.find((thread, i) => threadKey(thread, i) === activeKey) ??
    bestThread;
  const matchedException = matchExceptionForThread(exception, activeThread);

  const originalFrames =
    matchedException?.stacktrace?.frames ??
    activeThread?.stacktrace?.frames ??
    [];
  const displayFrames = orderFramesForDisplay(originalFrames, platform);

  const copyFormats = matchedException
    ? [
        {
          label: t('copy.plainText'),
          value: formatStackTraceAsText(
            { values: [matchedException] },
            platform,
          ),
        },
        {
          label: t('copy.json'),
          value: JSON.stringify(matchedException, null, 2),
        },
      ]
    : [{ label: t('copy.json'), value: JSON.stringify(activeThread, null, 2) }];

  const threadItems = Object.fromEntries(
    threads.map((thread, i) => [
      threadKey(thread, i),
      `${threadLabel(thread, t)}${thread.crashed ? t('threads.crashedSuffix') : ''}`,
    ]),
  );

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        {threads.length > 1 ? (
          <Select
            value={activeKey}
            onValueChange={(value) => {
              if (value) setActiveKey(value);
            }}
            items={threadItems}
            aria-label={t('threads.thread')}
            className="w-65"
          >
            {Object.entries(threadItems).map(([key, label]) => (
              <SelectItem key={key} value={key}>
                {label}
              </SelectItem>
            ))}
          </Select>
        ) : (
          <p className="font-semibold text-body text-fg">
            {activeThread ? threadLabel(activeThread, t) : t('threads.thread')}
          </p>
        )}

        <div className="flex items-center gap-2">
          {activeThread?.crashed && (
            <Tag tone="error" variant="soft">
              {t('threads.crashed')}
            </Tag>
          )}
          {activeThread?.current && (
            <Tag variant="soft">{t('threads.current')}</Tag>
          )}
          {activeThread?.main && <Tag variant="soft">{t('threads.main')}</Tag>}
          {activeThread?.state && (
            <Tag variant="soft">{activeThread.state}</Tag>
          )}
          <CopyAsDropdown formats={copyFormats} />
        </div>
      </div>

      {matchedException && (
        <div className="space-y-1">
          <h3 className="text-section text-sev-error-fg">
            {matchedException.type}
          </h3>
          <p className="text-body text-fg-muted">{matchedException.value}</p>
        </div>
      )}

      {displayFrames.length > 0 ? (
        <div className="space-y-2">
          {displayFrames.map((frame) => {
            const originalIdx = originalFrames.indexOf(frame);
            return (
              <StackFrameItem
                key={originalIdx}
                frame={frame}
                index={originalIdx + 1}
                sourceUrl={sourceUrl}
              />
            );
          })}
        </div>
      ) : (
        <div className="py-12 text-center text-body text-fg-muted">
          {t('stackTrace.emptyForThread')}
        </div>
      )}
    </div>
  );
}
