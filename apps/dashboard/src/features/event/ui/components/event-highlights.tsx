import type { EventDetailResponse } from '@rustrak/client';
import { useTranslations } from 'use-intl';
import { cn } from '@/shared/lib/utils';

interface EventHighlightsProps {
  event: EventDetailResponse;
  tags: Record<string, string>;
  contexts?: Record<string, Record<string, unknown>>;
  eventData: Record<string, unknown>;
}

function asRecord(v: unknown): Record<string, unknown> | undefined {
  return v && typeof v === 'object'
    ? (v as Record<string, unknown>)
    : undefined;
}

function str(v: unknown): string | undefined {
  if (v === null || v === undefined) return undefined;
  if (typeof v === 'boolean') return v ? 'yes' : 'no';
  const s = String(v);
  return s.length > 0 ? s : undefined;
}

interface Highlight {
  labelKey: string;
  value?: string;
  mono?: boolean;
}

/**
 * Read-only "Highlights" panel: the handful of promoted attributes Sentry shows
 * at a glance, synthesized from the event's fields, tags and contexts.
 */
export function EventHighlights({
  event,
  tags,
  contexts,
  eventData,
}: EventHighlightsProps) {
  const t = useTranslations('events');
  const firstException = asRecord(
    (asRecord(eventData.exception)?.values as unknown[] | undefined)?.[0],
  );
  const mechanism = asRecord(firstException?.mechanism);
  const request = asRecord(eventData.request);
  const runtime = asRecord(contexts?.runtime);
  const trace = asRecord(contexts?.trace);

  const runtimeLabel =
    [str(runtime?.name), str(runtime?.version)].filter(Boolean).join(' ') ||
    tags['runtime.name'];

  const items: Highlight[] = [
    {
      labelKey: 'highlights.handled',
      value: tags.handled ?? str(mechanism?.handled),
    },
    { labelKey: 'highlights.level', value: event.level ?? tags.level },
    {
      labelKey: 'highlights.transaction',
      value: str(eventData.transaction) ?? tags.transaction,
      mono: true,
    },
    {
      labelKey: 'highlights.url',
      value: str(request?.url) ?? tags.url,
      mono: true,
    },
    {
      labelKey: 'highlights.environment',
      value: event.environment ?? tags.environment,
    },
    { labelKey: 'highlights.release', value: event.release, mono: true },
    // A release's build per target (Android versionCode, Apple build number).
    { labelKey: 'highlights.dist', value: str(eventData.dist), mono: true },
    { labelKey: 'highlights.runtime', value: runtimeLabel || undefined },
    { labelKey: 'highlights.traceId', value: str(trace?.trace_id), mono: true },
  ].filter((i) => Boolean(i.value));

  if (items.length === 0) {
    return null;
  }

  const mid = Math.ceil(items.length / 2);
  const columns = [items.slice(0, mid), items.slice(mid)];

  return (
    <div className="grid grid-cols-1 md:grid-cols-2 gap-x-8">
      {columns.map((col, idx) => (
        <dl key={idx === 0 ? 'left' : 'right'} className="text-sm">
          {col.map((it) => (
            <div
              key={it.labelKey}
              className="flex items-center gap-4 rounded px-2 py-1.5 odd:bg-surface-hover/40"
            >
              <dt className="w-28 shrink-0 text-fg-subtle">{t(it.labelKey)}</dt>
              <dd
                className={cn(
                  'min-w-0 truncate',
                  it.mono && 'font-mono text-xs',
                )}
              >
                {it.value}
              </dd>
            </div>
          ))}
        </dl>
      ))}
    </div>
  );
}
