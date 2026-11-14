import type { LogResponse } from '@rustrak/client';
import {
  BackIcon,
  Button,
  CloseIcon,
  FilterIcon,
  Tag,
  Text,
} from '@rustrak/ui';
import type { ReactNode } from 'react';
import { useFormatter, useTranslations } from 'use-intl';
import { levelTone } from '@/shared/config/levels';
import { useRelativeTime } from '@/shared/i18n/use-now';

/**
 * One log line in full, beside the list: the message unwrapped, when it
 * happened and how bad it was, the trace it belongs to -- one click narrows
 * the list to that trace -- and every attribute the SDK sent with it.
 */
export function LogDetail({
  log,
  onTrace,
  onClose,
}: {
  log: LogResponse;
  /** Narrows the list to the logs of this line's trace. */
  onTrace: (traceId: string) => void;
  onClose: () => void;
}) {
  const t = useTranslations('logs');
  const format = useFormatter();
  const relativeTime = useRelativeTime();
  const at = new Date(log.timestamp);
  const attributes = Object.entries(
    (log.attributes ?? {}) as Record<string, unknown>,
  ).sort(([a], [b]) => a.localeCompare(b));

  return (
    <article className="flex min-h-0 flex-1 flex-col overflow-y-auto">
      <header className="flex items-center gap-2 border-border-divider border-b py-3 lg:px-4">
        <Button
          variant="ghost"
          size="sm"
          icon={BackIcon}
          aria-label={t('back')}
          className="lg:hidden"
          onClick={onClose}
        />
        <Tag variant="soft" tone={levelTone(log.level) ?? 'neutral'}>
          {log.level}
        </Tag>
        <time
          dateTime={log.timestamp}
          className="min-w-0 flex-1 truncate font-mono text-fg-secondary text-mono-sm tabular-nums"
        >
          {format.dateTime(at, 'logLine')}
        </time>
        <Button
          variant="ghost"
          size="sm"
          icon={CloseIcon}
          shortcut="Esc"
          aria-label={t('close')}
          className="max-lg:hidden"
          onClick={onClose}
        />
      </header>

      <div className="flex flex-col gap-5 py-4 lg:px-4">
        <pre className="whitespace-pre-wrap break-words rounded-lg border border-border bg-surface p-3 font-mono text-fg text-mono-sm">
          {log.body || t('emptyValue')}
        </pre>

        <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-6 gap-y-2 text-body">
          <Fact label={t('time')}>
            <span className="text-fg-secondary">
              {format.dateTime(at, 'precise')}
              <span className="text-fg-subtle">
                {' · '}
                {relativeTime(at)}
              </span>
            </span>
          </Fact>
          {log.severity_number !== null ? (
            <Fact label={t('severity')}>
              <span className="font-mono text-fg-secondary text-mono-sm">
                {log.severity_number}
              </span>
            </Fact>
          ) : null}
          {log.trace_id ? (
            <Fact label={t('trace')}>
              <span className="flex min-w-0 items-center gap-2">
                <span className="truncate font-mono text-fg-secondary text-mono-sm">
                  {log.trace_id}
                </span>
                <Button
                  size="sm"
                  variant="ghost"
                  icon={FilterIcon}
                  onClick={() => log.trace_id && onTrace(log.trace_id)}
                >
                  {t('sameTrace')}
                </Button>
              </span>
            </Fact>
          ) : null}
          {log.span_id ? (
            <Fact label={t('span')}>
              <span className="font-mono text-fg-secondary text-mono-sm">
                {log.span_id}
              </span>
            </Fact>
          ) : null}
        </dl>

        {attributes.length > 0 ? (
          <section className="flex flex-col gap-2">
            <Text variant="meta" tone="subtle">
              {t('attributes', { count: attributes.length })}
            </Text>
            <dl className="divide-y divide-border-divider rounded-lg border border-border">
              {attributes.map(([key, raw]) => (
                <div
                  key={key}
                  className="grid grid-cols-[minmax(0,2fr)_minmax(0,3fr)] gap-3 px-3 py-1.5 font-mono text-mono-sm"
                >
                  <dt className="truncate text-fg-subtle" title={key}>
                    {key}
                  </dt>
                  <dd className="break-all text-fg">{attributeValue(raw)}</dd>
                </div>
              ))}
            </dl>
          </section>
        ) : null}
      </div>
    </article>
  );
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <dt className="text-fg-subtle">{label}</dt>
      <dd className="min-w-0">{children}</dd>
    </>
  );
}

/**
 * One attribute as text. Logs carry the OTel-style typed map
 * (`{ value, type }`); anything else is shown as JSON.
 */
function attributeValue(raw: unknown): string {
  const value =
    raw !== null && typeof raw === 'object' && 'value' in raw
      ? (raw as { value: unknown }).value
      : raw;
  return typeof value === 'string' ? value : JSON.stringify(value);
}
