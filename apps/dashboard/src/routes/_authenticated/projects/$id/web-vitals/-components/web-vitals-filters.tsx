import { NavigationIcon, SegmentedControl, SegmentedItem } from '@rustrak/ui';
import { useQuery } from '@tanstack/react-query';
import { useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import {
  DEFAULT_PERIOD,
  PERFORMANCE_PERIODS,
  type PerformancePeriod,
  performancePeriod,
  periodParam,
} from '@/shared/lib/period';
import { scopeActions } from '@/shared/lib/scope-actions';
import { searchOneOf, searchString } from '@/shared/lib/search-params';
import {
  EnvironmentSegment,
  PeriodSegment,
} from '@/shared/ui/components/period-filter';
import { ScopeBar, ScopeSegment } from '@/shared/ui/components/scope-bar';

const DEVICES = ['mobile', 'desktop'] as const;
type Device = (typeof DEVICES)[number];

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

/**
 * Phones, computers or both, as PageSpeed and Vercel put it: a toggle in
 * sight rather than a filter in a menu, since each device has its own
 * story and a mixed p75 hides the slower one.
 */
export function DeviceToggle({
  device,
  onChange,
}: {
  device?: Device;
  onChange: (device: Device | undefined) => void;
}) {
  const tDevices = useTranslations('webVitals.devices');
  return (
    <SegmentedControl
      aria-label={tDevices('device')}
      value={device ?? 'all'}
      onValueChange={(value) =>
        onChange(value === 'all' ? undefined : (value as Device))
      }
    >
      <SegmentedItem value="all">{tDevices('all')}</SegmentedItem>
      {DEVICES.map((value) => (
        <SegmentedItem key={value} value={value}>
          {tDevices(value)}
        </SegmentedItem>
      ))}
    </SegmentedControl>
  );
}

/**
 * The slice every panel reads: the environment, the browser (they score
 * on their own curves, and Safari reports neither LCP nor CLS) and the
 * window.
 */
export function WebVitalsFilters({
  projectId,
  scope,
  browsers,
  held = false,
  onChange,
}: {
  projectId: number;
  scope: WebVitalsScope;
  /** The browsers the window's vitals came from. */
  browsers: string[];
  /** In the skeleton: its choices are not asked for. */
  held?: boolean;
  onChange: (change: WebVitalsScope) => void;
}) {
  const t = useTranslations('webVitals');
  const environments =
    useQuery({
      ...projectQueries.environments(projectId),
      enabled: !held,
      subscribed: !held,
    }).data ?? [];

  return (
    <ScopeBar>
      <EnvironmentSegment
        environments={environments}
        environment={scope.environment}
        onChange={(value) => onChange({ environment: value })}
      />
      <ScopeSegment
        icon={NavigationIcon}
        label={scope.browser ?? t('browser')}
        active={scope.browser !== undefined}
        actions={scopeActions(
          browsers,
          scope.browser,
          t('allBrowsers'),
          t('noBrowsers'),
          (value) => onChange({ browser: value }),
        )}
      />
      <PeriodSegment
        choices={PERFORMANCE_PERIODS}
        period={scope.period ?? DEFAULT_PERIOD}
        onChange={(value) => onChange({ period: value })}
      />
    </ScopeBar>
  );
}

/**
 * A Web Vitals page's header actions, the same on each of them: the device
 * toggle, then the filters.
 */
export function ScopeActions({
  projectId,
  scope,
  browsers,
  held,
  onChange,
}: {
  projectId: number;
  scope: WebVitalsScope;
  browsers: string[];
  held?: boolean;
  onChange: (change: WebVitalsScope) => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-2">
      <DeviceToggle
        device={scope.device}
        onChange={(device) => onChange({ device })}
      />
      <WebVitalsFilters
        projectId={projectId}
        scope={scope}
        browsers={browsers}
        held={held}
        onChange={onChange}
      />
    </div>
  );
}
