/**
 * The slice a Web Vitals page reads, in and out of the address. Kept apart
 * from the filters' components, so a file holds components or helpers, not
 * both.
 */
import {
  DEFAULT_PERIOD,
  type PerformancePeriod,
  performancePeriod,
  periodParam,
} from '@/shared/lib/period';
import { searchOneOf, searchString } from '@/shared/lib/search-params';

export const DEVICES = ['mobile', 'desktop'] as const;
export type Device = (typeof DEVICES)[number];

/** The slice every panel of a Web Vitals page reads, as the address holds it. */
export interface WebVitalsScope {
  period?: PerformancePeriod;
  environment?: string;
  browser?: string;
  device?: Device;
}

/** `WebVitalsScope` out of the address, dropping what it cannot read. */
export function validateScope(search: Record<string, unknown>): WebVitalsScope {
  return {
    period: performancePeriod(search.period),
    environment: searchString(search.environment),
    browser: searchString(search.browser),
    device: searchOneOf(search.device, DEVICES),
  };
}

/** The scope as the API reads it. */
export function apiScope(scope: WebVitalsScope) {
  return {
    period: scope.period ?? DEFAULT_PERIOD,
    environment: scope.environment,
    browser: scope.browser,
    device: scope.device,
  };
}

/** A change of scope as the address writes it: the defaults left out. */
export function scopeChange(change: WebVitalsScope): WebVitalsScope {
  return 'period' in change
    ? { ...change, period: periodParam(change.period ?? DEFAULT_PERIOD) }
    : change;
}
