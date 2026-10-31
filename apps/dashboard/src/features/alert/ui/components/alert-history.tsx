import type { AlertHistory, AlertHistoryDay } from '@rustrak/client';
import { BarsChart, cn, Tag, type TagTone, Text } from '@rustrak/ui';
import { useFormatter, useTranslations } from 'use-intl';
import { Link } from '@/shared/ui/components/link';
import { ProviderIcon } from './provider-icon';

const CELL = 'px-4 py-2.5 align-top';

const STATUS_TONE: Record<AlertHistory['status'], TagTone> = {
  sent: 'brand',
  failed: 'error',
  pending: 'neutral',
  skipped: 'warning',
};

/** Deliveries per UTC day, failures in red. */
export function HistoryChart({ days }: { days: AlertHistoryDay[] }) {
  const t = useTranslations('alerts.history');
  const format = useFormatter();
  return (
    <BarsChart
      data={days.map((d) => ({ ...d, t: Date.parse(d.day) }))}
      series={[
        { key: 'sent', label: t('status.sent'), color: 'var(--chart-1)' },
        {
          key: 'skipped',
          label: t('status.skipped'),
          color: 'var(--sev-warning)',
        },
        { key: 'failed', label: t('status.failed'), color: 'var(--sev-error)' },
      ]}
      xKey="t"
      height={180}
      formatX={(value) => format.dateTime(new Date(value), 'axisDayUtc')}
      formatY={(value) => format.number(value)}
      label={t('chart')}
    />
  );
}

/**
 * Every delivery, newest first: how it went, which alert sent it through
 * which channel, about which issue, why it failed or was skipped, and when.
 * What answers "why did I not get an alert?".
 */
export function HistoryTable({
  projectId,
  history,
  ruleNames,
  ruleHref,
}: {
  projectId: number;
  history: AlertHistory[];
  /** With it, a column for the alert each delivery came from. */
  ruleNames?: Map<number, string>;
  ruleHref?: (ruleId: number) => string;
}) {
  const t = useTranslations('alerts.history');

  if (history.length === 0) {
    return (
      <Text variant="meta" tone="subtle" className="px-4 py-10 text-center">
        {t('empty')}
      </Text>
    );
  }

  const cell = CELL;
  return (
    <div className="min-h-0 flex-1 overflow-auto">
      <table className="w-full text-meta">
        <thead className="sticky top-0 bg-panel">
          <tr className="border-border-divider border-b text-left font-mono text-column text-fg-meta uppercase">
            <th className={cn(cell, 'w-28 font-normal')}>
              {t('columns.status')}
            </th>
            {ruleNames ? (
              <th className={cn(cell, 'font-normal')}>{t('columns.alert')}</th>
            ) : null}
            <th className={cn(cell, 'font-normal')}>{t('columns.channel')}</th>
            <th className={cn(cell, 'w-[40%] font-normal max-md:hidden')}>
              {t('columns.detail')}
            </th>
            <th className={cn(cell, 'w-36 text-right font-normal')}>
              {t('columns.when')}
            </th>
          </tr>
        </thead>
        <tbody className="divide-y divide-border-divider">
          {history.map((row) => (
            <HistoryRow
              key={row.id}
              projectId={projectId}
              row={row}
              ruleNames={ruleNames}
              ruleHref={ruleHref}
            />
          ))}
        </tbody>
      </table>
    </div>
  );
}

function HistoryRow({
  projectId,
  row,
  ruleNames,
  ruleHref,
}: {
  projectId: number;
  row: AlertHistory;
  ruleNames?: Map<number, string>;
  ruleHref?: (ruleId: number) => string;
}) {
  const t = useTranslations('alerts.history');
  const format = useFormatter();
  const cell = CELL;
  const at = new Date(row.created_at);
  const rule = row.alert_rule_id;
  return (
    <tr key={row.id}>
      <td className={cell}>
        <Tag variant="soft" tone={STATUS_TONE[row.status]}>
          {t(`status.${row.status}`)}
        </Tag>
      </td>
      {ruleNames ? (
        <td className={cn(cell, 'text-fg')}>
          {rule !== null && ruleNames.has(rule) && ruleHref ? (
            <Link href={ruleHref(rule)} className="hover:underline">
              {ruleNames.get(rule)}
            </Link>
          ) : (
            <span className="text-fg-subtle">{t('deletedAlert')}</span>
          )}
        </td>
      ) : null}
      <td className={cell}>
        <span className="flex items-center gap-1.5 text-fg-secondary">
          <ProviderIcon
            type={row.channel_type}
            className="size-3.5 shrink-0 text-fg-subtle"
          />
          {row.channel_name}
        </span>
      </td>
      <td className={cn(cell, 'max-md:hidden')}>
        <span className="flex flex-col gap-0.5">
          {row.issue_id ? (
            <Link
              href={`/projects/${projectId}/issues/${row.issue_id}`}
              className="text-fg-secondary hover:text-fg hover:underline"
            >
              {t('openIssue')}
            </Link>
          ) : null}
          {row.error_message ? (
            <span
              className={cn(
                'break-words font-mono text-mono-sm',
                row.status === 'failed'
                  ? 'text-sev-error-fg'
                  : 'text-fg-subtle',
              )}
            >
              {row.http_status_code ? `HTTP ${row.http_status_code} · ` : null}
              {row.error_message}
            </span>
          ) : null}
        </span>
      </td>
      <td className={cn(cell, 'text-right text-fg-subtle tabular-nums')}>
        <time dateTime={row.created_at} title={format.dateTime(at, 'precise')}>
          {format.relativeTime(at)}
        </time>
      </td>
    </tr>
  );
}
