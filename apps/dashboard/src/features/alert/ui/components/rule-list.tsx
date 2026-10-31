import type {
  AlertIntegration,
  AlertRuleResponse,
  AlertType,
} from '@rustrak/client';
import { cn, focusRingInset, Skeleton, Switch, Text } from '@rustrak/ui';
import { useFormatter, useTranslations } from 'use-intl';
import {
  alertHref,
  cooldownUnit,
  newAlertHref,
  ruleShape,
} from '@/features/alert/model/rule-summary';
import { Link } from '@/shared/ui/components/link';
import { AlertTypeIcon } from './alert-type-icon';
import { ProviderIcon } from './provider-icon';

const COLUMNS =
  'lg:grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)_minmax(0,1fr)_minmax(0,1.2fr)_8rem_8rem_3rem]';

/** How often an alert may repeat, in a word or two. */
export function useFrequency() {
  const t = useTranslations('alerts');
  return (minutes: number) => {
    const unit = cooldownUnit(minutes);
    return unit
      ? t(`editor.every.${unit.unit}`, { count: unit.value })
      : t('editor.everyTime');
  };
}

/** The filters of a rule, as short labels: environments, level, events. */
export function useConditionLabels() {
  const t = useTranslations('alerts');
  return (shape: ReturnType<typeof ruleShape>) => [
    ...(shape.environments.length
      ? shape.environments
      : [t('editor.allEnvironments')]),
    ...(shape.minLevel !== 'any'
      ? [t('editor.levelOrHigher', { level: shape.minLevel })]
      : []),
    ...(shape.type === 'new_issue' && shape.minEvents > 1
      ? [t('table.afterEvents', { count: shape.minEvents })]
      : []),
  ];
}

/**
 * Every alert of the project, a row each: what it is called and fires on,
 * what narrows it, whom it tells and how often, when it last sent, and a
 * switch to pause it. Alerts are independent -- two can fire on new issues
 * with different filters and channels -- so this is just the list.
 */
export function RulesTable({
  projectId,
  rules,
  integrations,
  busy,
  onToggle,
}: {
  projectId: number;
  /** Absent while the rules load: the table draws skeleton rows. */
  rules?: AlertRuleResponse[];
  integrations: AlertIntegration[];
  busy: boolean;
  onToggle: (rule: AlertRuleResponse, enabled: boolean) => void;
}) {
  const t = useTranslations('alerts');
  const format = useFormatter();
  const frequency = useFrequency();
  const conditions = useConditionLabels();

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-border bg-panel">
      <div
        className={cn(
          'hidden shrink-0 gap-4 border-border-divider border-b px-4 py-2.5 font-mono text-column text-fg-meta uppercase lg:grid',
          COLUMNS,
        )}
      >
        <span>{t('table.alert')}</span>
        <span>{t('table.when')}</span>
        <span>{t('table.conditions')}</span>
        <span>{t('table.notifies')}</span>
        <span>{t('table.frequency')}</span>
        <span className="text-right">{t('table.lastSent')}</span>
        <span className="sr-only">{t('table.active')}</span>
      </div>
      <ul className="min-h-0 flex-1 divide-y divide-border-divider overflow-y-auto">
        {rules ? null : <RuleRowsSkeleton />}
        {rules?.map((rule) => {
          const shape = ruleShape(rule, integrations);
          const channels = integrations.filter((i) =>
            rule.integration_ids.includes(i.id),
          );
          return (
            <li key={rule.id} className="relative">
              <Link
                href={alertHref(projectId, rule.id)}
                className={cn(
                  'grid grid-cols-1 gap-x-4 gap-y-1.5 py-3 ps-4 pe-16 transition-colors hover:bg-surface-hover lg:items-center lg:pe-4',
                  COLUMNS,
                  focusRingInset,
                  !rule.is_enabled && 'opacity-60',
                )}
              >
                <span className="flex min-w-0 items-center gap-3">
                  <span
                    aria-hidden="true"
                    className={cn(
                      'h-8 w-0.75 shrink-0 rounded-pill',
                      rule.is_enabled ? 'bg-success' : 'bg-border-control',
                    )}
                  />
                  <span className="truncate font-medium text-body text-fg">
                    {rule.name}
                  </span>
                </span>
                <span className="flex min-w-0 items-center gap-2 text-fg-secondary text-meta">
                  <AlertTypeIcon
                    type={rule.alert_type}
                    className="size-3.5 shrink-0 text-fg-subtle"
                  />
                  <span className="truncate">
                    {t(`types.${rule.alert_type}.name`)}
                  </span>
                </span>
                <span className="truncate text-fg-secondary text-meta">
                  {conditions(shape).join(' · ')}
                </span>
                <span className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-fg-secondary text-meta">
                  {channels.map((channel) => (
                    <span
                      key={channel.id}
                      className="flex min-w-0 items-center gap-1.5"
                    >
                      <ProviderIcon
                        type={channel.provider_type}
                        className="size-3.5 shrink-0 text-fg-subtle"
                      />
                      <span className="truncate">{channel.name}</span>
                    </span>
                  ))}
                </span>
                <span className="text-fg-secondary text-meta">
                  {frequency(rule.cooldown_minutes)}
                </span>
                <span className="text-fg-subtle text-meta tabular-nums lg:text-right">
                  {rule.last_triggered_at
                    ? format.relativeTime(new Date(rule.last_triggered_at))
                    : t('list.neverTriggered')}
                </span>
              </Link>
              {/* Outside the link: a switch inside an anchor is a control
                  inside a control. Placed over the row's last column. */}
              <span className="absolute top-3 right-4 lg:top-1/2 lg:-translate-y-1/2">
                <Switch
                  checked={rule.is_enabled}
                  disabled={busy}
                  aria-label={t('list.enabled', { name: rule.name })}
                  onCheckedChange={(enabled) => onToggle(rule, enabled)}
                />
              </span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

/** Rows in the table's columns, at the size of what replaces them. */
function RuleRowsSkeleton() {
  return Array.from({ length: 5 }, (_, row) => (
    <li
      // Placeholder rows have no identity beyond their position.
      // react-doctor-disable-next-line react-doctor/no-array-index-as-key
      key={row}
      className={cn(
        'grid grid-cols-1 gap-x-4 gap-y-1.5 py-3 ps-4 pe-16 lg:items-center lg:pe-4',
        COLUMNS,
      )}
    >
      <span className="flex items-center gap-3">
        <Skeleton className="h-8 w-0.75" />
        <Skeleton className="h-4 w-40" />
      </span>
      <Skeleton className="h-3 w-24" />
      <Skeleton className="h-3 w-32" />
      <Skeleton className="h-3 w-28" />
      <Skeleton className="h-3 w-20" />
      <Skeleton className="h-3 w-16 lg:ms-auto" />
    </li>
  ));
}

/** Ways to start, for a project with no alert yet: one per trigger. */
export function AlertStarters({ projectId }: { projectId: number }) {
  const t = useTranslations('alerts');
  const starters: AlertType[] = [
    'new_issue',
    'regression',
    'unmute',
    'new_feedback',
  ];
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-1">
        <Text variant="body">{t('overview.emptyTitle')}</Text>
        <Text variant="meta" tone="subtle">
          {t('overview.emptyDescription')}
        </Text>
      </div>
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        {starters.map((type) => (
          <Link
            key={type}
            href={newAlertHref(projectId, type)}
            className={cn(
              'flex items-start gap-3 rounded-lg border border-border bg-panel p-4 transition-colors hover:bg-surface-hover',
              focusRingInset,
            )}
          >
            <AlertTypeIcon
              type={type}
              className="mt-0.5 size-4 shrink-0 text-fg-secondary"
            />
            <span className="flex flex-col gap-0.5">
              <span className="font-medium text-body text-fg">
                {t(`starters.${type}`)}
              </span>
              <span className="text-fg-subtle text-meta">
                {t(`types.${type}.description`)}
              </span>
            </span>
          </Link>
        ))}
      </div>
    </div>
  );
}
