/**
 * One severity scale for every level the app shows: an issue's, an event's,
 * a breadcrumb's and a log line's (`warn` is how log lines spell it).
 */
type Severity = 'error' | 'warning' | 'info';

const SEVERITY: Record<string, Severity> = {
  fatal: 'error',
  error: 'error',
  warning: 'warning',
  warn: 'warning',
  info: 'info',
};

/** A level's tone, as a `Tag` takes it; none for debug, trace or unknown. */
export function levelTone(level?: string | null): Severity | undefined {
  return SEVERITY[level ?? ''];
}

const MARK: Record<Severity, string> = {
  error: 'bg-sev-error',
  warning: 'bg-sev-warning',
  info: 'bg-sev-info',
};

/** The colour of a level's stripe or dot. */
export function levelMark(
  level?: string | null,
  fallback = 'bg-sev-info',
): string {
  const tone = levelTone(level);
  return tone ? MARK[tone] : fallback;
}

const TEXT: Record<Severity, string> = {
  error: 'text-sev-error-fg',
  warning: 'text-sev-warning-fg',
  info: 'text-fg-subtle',
};

/** The colour of a level's icon or word. */
export function levelText(level?: string | null): string {
  return TEXT[levelTone(level) ?? 'info'];
}
