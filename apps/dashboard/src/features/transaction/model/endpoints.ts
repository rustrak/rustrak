/**
 * How a failure rate reads: a look from 1%, a problem from 5%. Under 1%
 * stays quiet, so a column of healthy endpoints is not a column of colour.
 */
export function failureTone(rate: number): 'warning' | 'bad' | null {
  if (rate >= 0.05) return 'bad';
  if (rate >= 0.01) return 'warning';
  return null;
}

/** The text colour of each `failureTone`. */
export const FAILURE_TEXT = {
  warning: 'text-sev-warning-fg',
  bad: 'text-sev-error-fg',
} as const;

/** Calls per minute over a window of `hours`. */
export function perMinute(count: number, hours: number): number {
  return count / (hours * 60);
}

/** The full page of one endpoint, keeping the window it was read in. */
export function endpointHref(
  projectId: number,
  endpoint: { name: string; op: string | null },
  scope: { period?: string; environment?: string } = {},
): string {
  const params = new URLSearchParams({ name: endpoint.name });
  if (endpoint.op) params.set('op', endpoint.op);
  if (scope.period) params.set('period', scope.period);
  if (scope.environment) params.set('environment', scope.environment);
  return `/projects/${projectId}/performance/summary?${params}`;
}

/** How many hours a window like `24h` or `7d` spans; none for all time. */
export function windowHours(period: string): number | undefined {
  const match = /^(\d+)([hd])$/.exec(period);
  if (!match) return undefined;
  return Number(match[1]) * (match[2] === 'd' ? 24 : 1);
}
