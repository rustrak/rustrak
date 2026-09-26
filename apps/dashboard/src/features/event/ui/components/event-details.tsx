import type { EventDetail } from '@rustrak/client';
import { Check, Copy } from 'lucide-react';
import { useState } from 'react';
import { toast } from 'sonner';
import { useFormatter, useTranslations } from 'use-intl';
import { copyToClipboard } from '@/shared/lib/clipboard';
import { Button } from '@/shared/ui/components/shadcn/button';

interface EventDetailsProps {
  event: EventDetail;
}

interface DetailRowProps {
  label: string;
  value: string | number | boolean | null | undefined;
  mono?: boolean;
  copyable?: boolean;
}

function DetailRow({
  label,
  value,
  mono = false,
  copyable = false,
}: DetailRowProps) {
  const t = useTranslations('events');
  const common = useTranslations('common');
  const [copied, setCopied] = useState(false);

  if (value === null || value === undefined || value === '') return null;

  const displayValue =
    typeof value === 'boolean'
      ? value
        ? t('details.yes')
        : t('details.no')
      : String(value);

  const handleCopy = async () => {
    if (!(await copyToClipboard(displayValue))) {
      toast.info(common('clipboardUnavailable'), {
        description: t('clipboardValueHint'),
      });
      return;
    }

    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="flex items-start gap-4 py-2 border-b border-dotted last:border-b-0">
      <span className="w-1/4 text-sm text-muted-foreground shrink-0">
        {label}
      </span>
      <div className="flex-1 flex items-center gap-2 min-w-0">
        <span
          className={`text-sm break-all ${mono ? 'font-mono text-xs' : ''}`}
        >
          {displayValue}
        </span>
        {copyable && (
          <Button
            variant="ghost"
            size="sm"
            onClick={handleCopy}
            className="h-6 w-6 p-0 shrink-0"
          >
            {copied ? (
              <Check className="size-3 text-primary" />
            ) : (
              <Copy className="size-3" />
            )}
          </Button>
        )}
      </div>
    </div>
  );
}

interface SectionProps {
  title: string;
  children: React.ReactNode;
}

function Section({ title, children }: SectionProps) {
  return (
    <div className="space-y-2">
      <h4 className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
        {title}
      </h4>
      <div className="bg-card rounded-lg border p-4">{children}</div>
    </div>
  );
}

/** The parts of a Sentry event payload these sections read. */
interface EventPayload {
  logentry?: { message?: string; formatted?: string; params?: unknown };
  request?: {
    method?: string;
    url?: string;
    headers?: Record<string, string>;
    env?: Record<string, string>;
    query_string?: string;
  };
  modules?: Record<string, string>;
  extra?: Record<string, unknown>;
  transaction?: string;
  exception?: {
    values?: Array<{ mechanism?: { type?: string; handled?: boolean } }>;
  };
}

function isNonEmpty<T extends object>(record: T | undefined): record is T {
  return record !== undefined && Object.keys(record).length > 0;
}

export function EventDetails({ event }: EventDetailsProps) {
  const payload = event.data as EventPayload;

  return (
    <div className="space-y-6">
      <KeyInformation event={event} payload={payload} />
      <LogEntry logentry={payload.logentry} />
      <Deployment event={event} />
      <Sdk event={event} />
      <RequestInfo request={payload.request} />
      <Modules modules={payload.modules} />
      <ExtraData extra={payload.extra} />
    </div>
  );
}

function KeyInformation({
  event,
  payload,
}: {
  event: EventDetail;
  payload: EventPayload;
}) {
  const format = useFormatter();
  const t = useTranslations('events');
  const mechanism = payload.exception?.values?.[0]?.mechanism;

  return (
    <Section title={t('details.keyInformation')}>
      <DetailRow
        label={t('details.eventId')}
        value={event.event_id}
        mono
        copyable
      />
      <DetailRow
        label={t('details.issueId')}
        value={event.issue_id}
        mono
        copyable
      />
      <DetailRow label={t('details.transaction')} value={payload.transaction} />
      <DetailRow
        label={t('details.timestamp')}
        value={format.dateTime(new Date(event.timestamp), 'precise')}
      />
      <DetailRow
        label={t('details.ingestedAt')}
        value={format.dateTime(new Date(event.ingested_at), 'precise')}
      />
      <DetailRow label={t('details.level')} value={event.level} />
      <DetailRow label={t('details.handled')} value={mechanism?.handled} />
      <DetailRow label={t('details.mechanism')} value={mechanism?.type} />
    </Section>
  );
}

function LogEntry({ logentry }: { logentry: EventPayload['logentry'] }) {
  const t = useTranslations('events');
  if (!logentry?.message && !logentry?.formatted) return null;

  return (
    <Section title={t('details.logEntry')}>
      <DetailRow label={t('details.message')} value={logentry.message} />
      <DetailRow label={t('details.formatted')} value={logentry.formatted} />
      <DetailRow
        label={t('details.params')}
        value={
          logentry.params == null
            ? undefined
            : JSON.stringify(logentry.params, null, 2)
        }
        mono
      />
    </Section>
  );
}

function Deployment({ event }: { event: EventDetail }) {
  const t = useTranslations('events');
  return (
    <Section title={t('details.deployment')}>
      <DetailRow label={t('details.platform')} value={event.platform} />
      <DetailRow label={t('details.environment')} value={event.environment} />
      <DetailRow label={t('details.release')} value={event.release} mono />
      <DetailRow label={t('details.serverName')} value={event.server_name} />
    </Section>
  );
}

function Sdk({ event }: { event: EventDetail }) {
  const t = useTranslations('events');
  if (!event.sdk_name && !event.sdk_version) return null;

  return (
    <Section title={t('details.sdk')}>
      <DetailRow label={t('details.name')} value={event.sdk_name} />
      <DetailRow label={t('details.version')} value={event.sdk_version} mono />
    </Section>
  );
}

function RequestInfo({ request }: { request: EventPayload['request'] }) {
  const t = useTranslations('events');
  if (!request?.method && !request?.url) return null;

  return (
    <Section title={t('details.request')}>
      <DetailRow label={t('details.method')} value={request.method} />
      <DetailRow label={t('details.url')} value={request.url} mono />
      <DetailRow
        label={t('details.queryString')}
        value={request.query_string}
        mono
      />
      <KeyValueGroup title={t('details.headers')} values={request.headers} />
      <KeyValueGroup title={t('details.environment')} values={request.env} />
    </Section>
  );
}

/** A titled run of name/value rows inside a section, when there are any. */
function KeyValueGroup({
  title,
  values,
}: {
  title: string;
  values: Record<string, string> | undefined;
}) {
  if (!isNonEmpty(values)) return null;

  return (
    <div className="mt-4">
      <p className="text-xs font-bold uppercase tracking-widest text-muted-foreground mb-2">
        {title}
      </p>
      {Object.entries(values).map(([key, value]) => (
        <DetailRow key={key} label={key} value={value} mono />
      ))}
    </div>
  );
}

function Modules({ modules }: { modules: EventPayload['modules'] }) {
  const t = useTranslations('events');
  if (!isNonEmpty(modules)) return null;

  return (
    <Section title={t('details.modules')}>
      <div className="max-h-64 overflow-auto">
        {Object.entries(modules).map(([name, version]) => (
          <DetailRow key={name} label={name} value={version} mono />
        ))}
      </div>
    </Section>
  );
}

function ExtraData({ extra }: { extra: EventPayload['extra'] }) {
  const t = useTranslations('events');
  if (!isNonEmpty(extra)) return null;

  return (
    <Section title={t('details.extraData')}>
      {Object.entries(extra).map(([key, value]) => (
        <DetailRow
          key={key}
          label={key}
          value={
            typeof value === 'object'
              ? JSON.stringify(value, null, 2)
              : String(value)
          }
          mono={typeof value === 'object'}
        />
      ))}
    </Section>
  );
}
