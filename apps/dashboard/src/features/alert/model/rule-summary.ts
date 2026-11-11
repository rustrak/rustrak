import type {
  AlertIntegration,
  AlertRuleResponse,
  AlertType,
} from '@rustrak/client';
import {
  environmentsOf,
  formFromConditions,
} from '@/shared/lib/rule-conditions';

/** The cooldowns offered as one click: every time, 5 minutes, an hour, a day. */
export const COOLDOWN_PRESETS = [0, 5, 60, 1440] as const;

/** The cooldown a new rule starts with: an hour, so a burst pages once. */
export const DEFAULT_COOLDOWN = 60;

/** A cooldown in the largest unit that divides it; `null` for none. */
export function cooldownUnit(
  minutes: number,
): { unit: 'minute' | 'hour' | 'day'; value: number } | null {
  if (minutes <= 0) return null;
  if (minutes % 1440 === 0) return { unit: 'day', value: minutes / 1440 };
  if (minutes % 60 === 0) return { unit: 'hour', value: minutes / 60 };
  return { unit: 'minute', value: minutes };
}

/**
 * A rule as the sentence reads it. A disabled integration is left out of
 * whom it notifies: dispatch skips it, so nothing is sent through it.
 */
export function ruleShape(
  rule: AlertRuleResponse,
  integrations: AlertIntegration[],
) {
  const filters = formFromConditions(rule.conditions);
  const linked = new Set(rule.integration_ids);
  return {
    type: rule.alert_type,
    environments: environmentsOf(rule.conditions),
    minLevel: filters.min_level,
    minEvents: filters.min_events,
    cooldownMinutes: rule.cooldown_minutes,
    channels: integrations
      .filter((i) => i.is_enabled && linked.has(i.id))
      .map((i) => i.name),
  };
}

export const alertsHref = (projectId: number) =>
  `/projects/${projectId}/alerts`;
export const alertHref = (projectId: number, ruleId: number) =>
  `${alertsHref(projectId)}/${ruleId}`;
export const newAlertHref = (projectId: number, type?: AlertType) =>
  `${alertsHref(projectId)}/new${type ? `?type=${type}` : ''}`;

/**
 * The integrations catalogue, or one provider's page, keeping the project it
 * was opened from so the way back to its alerts survives.
 */
export const channelsHref = (projectId?: number, provider?: string) =>
  `/settings/integrations${provider ? `/${provider}` : ''}${projectId ? `?projectId=${projectId}` : ''}`;
