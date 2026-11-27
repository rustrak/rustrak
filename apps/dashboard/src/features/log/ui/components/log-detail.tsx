import type { LogResponse } from '@rustrak/client';
import {
  Button,
  CopyIcon,
  FilterIcon,
  SelectedIcon,
  Tooltip,
} from '@rustrak/ui';
import type { ReactNode } from 'react';
import { useFormatter, useTranslations } from 'use-intl';
import { attributeGroups } from '@/features/log/model/log-view';
import { useRelativeTime } from '@/shared/i18n/use-now';
import { useCopy } from '@/shared/ui/hooks/use-copy';

/**
 * One log line in full, opened in place under its row, as Sentry's log
 * table does: the message unwrapped, a line of facts -- the trace and the
 * environment narrow the list in one click -- and every attribute the SDK
 * sent, grouped by namespace in columns so a line with many stays short.
 * It grows open rather than appearing, so the eye follows it.
 */
export function LogDetail({
  log,
  onTrace,
  onEnvironment,
}: {
  log: LogResponse;
  /** Narrows the list to the logs of this line's trace. */
  onTrace: (traceId: string) => void;
  /** Narrows the list to the logs of this line's environment. */
  onEnvironment: (environment: string) => void;
}) {
  const t = useTranslations('logs');
  const format = useFormatter();
  const relativeTime = useRelativeTime();
  const { copied, copy } = useCopy();
  const at = new Date(log.timestamp);
  const groups = attributeGroups(log.attributes);

  return (
    <div className="grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-200 ease-out starting:grid-rows-[0fr] starting:opacity-0">
      <div className="min-h-0 overflow-hidden">
        <div className="flex flex-col gap-3 border-border-divider border-t bg-surface px-4 py-3">
          <div className="flex items-start gap-3">
            <pre className="max-h-40 min-w-0 flex-1 overflow-y-auto whitespace-pre-wrap break-words font-mono text-fg text-mono-sm">
              {log.body || t('emptyValue')}
            </pre>
            <Button
              size="sm"
              variant="ghost"
              icon={copied ? SelectedIcon : CopyIcon}
              onClick={() => copy(JSON.stringify(log, null, 2))}
            >
              {t('copyJson')}
            </Button>
          </div>

          <dl className="grid grid-cols-[repeat(auto-fill,minmax(15rem,1fr))] gap-2">
            <Fact label={t('time')}>
              <span className="truncate" title={relativeTime(at)}>
                {format.dateTime(at, 'precise')}
              </span>
            </Fact>
            {log.environment ? (
              <Fact label={t('environment')}>
                <Narrow
                  label={t('sameEnvironment')}
                  onClick={() =>
                    log.environment && onEnvironment(log.environment)
                  }
                >
                  {log.environment}
                </Narrow>
              </Fact>
            ) : null}
            {log.trace_id ? (
              <Fact label={t('trace')}>
                <Narrow
                  label={t('sameTrace')}
                  onClick={() => log.trace_id && onTrace(log.trace_id)}
                >
                  {log.trace_id}
                </Narrow>
              </Fact>
            ) : null}
            {log.span_id ? (
              <Fact label={t('span')}>
                <span className="truncate">{log.span_id}</span>
              </Fact>
            ) : null}
          </dl>

          {groups.length > 0 ? (
            <section className="flex flex-col gap-2">
              <h3 className="flex items-center gap-1.5 text-fg-subtle text-meta">
                {t('attributes')}
                <span className="text-fg-ghost tabular-nums">
                  {groups.reduce((n, g) => n + g.attributes.length, 0)}
                </span>
              </h3>
              {/* Columns, not a grid: a short group lets the next one rise
                  into its place, so no group leaves a hole beside it. */}
              <div className="columns-1 gap-2 md:columns-2 xl:columns-3">
                {groups.map((group) => (
                  <section
                    key={group.name}
                    aria-label={group.name || t('ownAttributes')}
                    className="mb-2 break-inside-avoid overflow-hidden rounded-lg border border-border-subtle bg-panel"
                  >
                    <h4 className="flex h-8 items-center justify-between border-border-subtle border-b bg-surface px-3 font-mono text-fg-secondary text-mono-sm">
                      {group.name || t('ownAttributes')}
                      <span className="text-fg-ghost tabular-nums">
                        {group.attributes.length}
                      </span>
                    </h4>
                    <dl className="divide-y divide-border-divider">
                      {group.attributes.map((attribute) => (
                        <Attribute
                          key={attribute.key}
                          name={attribute.key}
                          group={group.name}
                          value={attribute.value}
                        />
                      ))}
                    </dl>
                  </section>
                ))}
              </div>
            </section>
          ) : null}
        </div>
      </div>
    </div>
  );
}

/** The box every fact sits in, all one size: a field. */
const CELL =
  'group flex h-8 min-w-0 items-center gap-1 rounded-md border border-border-subtle bg-panel pr-0.5 pl-2.5 font-mono text-fg text-mono-sm';

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex min-w-0 flex-col gap-1">
      <dt className="text-fg-subtle text-meta">{label}</dt>
      <dd className={CELL}>{children}</dd>
    </div>
  );
}

/** A value with the icon button that narrows the list to it. */
function Narrow({
  label,
  onClick,
  children,
}: {
  label: string;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <>
      <span className="min-w-0 flex-1 truncate">{children}</span>
      <Tooltip content={label}>
        <Button
          size="sm"
          variant="ghost"
          icon={FilterIcon}
          aria-label={label}
          onClick={onClick}
          className="opacity-0 focus-visible:opacity-100 group-hover:opacity-100"
        />
      </Tooltip>
    </>
  );
}

/**
 * One row of a group's card: the key without the group's name, which the
 * card's head says, the value, and a copy button on hover. Every row is one
 * height, so a card reads as a form, not as loose text.
 */
function Attribute({
  name,
  group,
  value,
}: {
  name: string;
  group: string;
  value: string;
}) {
  const common = useTranslations('common');
  const { copied, copy } = useCopy();
  return (
    <div className="group grid h-8 grid-cols-[minmax(0,2fr)_minmax(0,3fr)] items-center gap-3 pl-3 font-mono text-mono-sm hover:bg-surface-hover">
      <dt className="truncate text-fg-subtle" title={name}>
        {group ? name.slice(group.length + 1) : name}
      </dt>
      <dd className="flex min-w-0 items-center gap-1 pr-0.5 text-fg">
        <span className="min-w-0 flex-1 truncate" title={value}>
          {value}
        </span>
        <Button
          size="sm"
          variant="ghost"
          icon={copied ? SelectedIcon : CopyIcon}
          aria-label={common('copyLabel', { label: name })}
          onClick={() => copy(value)}
          className="opacity-0 focus-visible:opacity-100 group-hover:opacity-100"
        />
      </dd>
    </div>
  );
}
