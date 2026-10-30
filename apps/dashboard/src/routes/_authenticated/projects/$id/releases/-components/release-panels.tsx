import type {
  IssueResponse,
  ReleaseHealthRow,
  ReleaseSummary,
  SessionSummary,
  SessionsGetTimeseriesResponse,
} from '@rustrak/client';
import {
  ArrowRightIcon,
  BarsChart,
  Button,
  CopyIcon,
  cn,
  ExternalLinkIcon,
  Skeleton,
  Tag,
} from '@rustrak/ui';
import type { UseQueryResult } from '@tanstack/react-query';
import type { ReactNode } from 'react';
import { toast } from 'sonner';
import { useFormatter, useTranslations } from 'use-intl';
import { splitVersion } from '@/features/release/model/session-health';
import {
  AdoptionBar,
  HealthRate,
} from '@/features/release/ui/components/release-figures';
import type { ApiError } from '@/shared/api/query';
import { Link } from '@/shared/ui/components/link';
import { QueryState } from '@/shared/ui/components/query-state';

type Read<T> = UseQueryResult<T, ApiError>;

const LEVEL_MARK: Record<string, string> = {
  fatal: 'bg-sev-error',
  error: 'bg-sev-error',
  warning: 'bg-sev-warning',
};

// ── header ───────────────────────────────────────────────────────────────────

/**
 * Which release this is and where it came from: the version, big and
 * copyable, its package above it, and under it when it was created, whether
 * it was released and the commit it was built from. The page's scope sits
 * on the right, as the issue page's actions do.
 */
export function ReleaseHeader({
  release,
  scope,
}: {
  /** Absent while it loads: its lines are drawn as skeleton bars. */
  release?: ReleaseSummary;
  scope: ReactNode;
}) {
  return (
    <header className="@container shrink-0 border-border-divider border-b px-page-gutter pt-6 pb-5">
      <div className="flex flex-col gap-4 @4xl:flex-row @4xl:items-start @4xl:justify-between @4xl:gap-8">
        {release ? (
          <ReleaseTitle release={release} />
        ) : (
          <div className="flex min-w-0 flex-col gap-1">
            <Skeleton className="my-0.5 h-5 w-20" />
            <Skeleton className="my-1 h-7 w-56" />
            <Skeleton className="my-0.5 h-4 w-40" />
          </div>
        )}
        {scope}
      </div>
    </header>
  );
}

function ReleaseTitle({ release }: { release: ReleaseSummary }) {
  const t = useTranslations('releases.detail');
  const tReleases = useTranslations('releases');
  const format = useFormatter();
  const parts = splitVersion(release.version);
  const created = new Date(release.date_created);

  const copy = () => {
    navigator.clipboard
      ?.writeText(release.version)
      .then(() => toast.success(t('copied')))
      .catch(() => {});
  };

  return (
    <div className="flex min-w-0 flex-col gap-1">
      <div className="flex flex-wrap items-center gap-2">
        {parts.package ? (
          <span className="font-mono text-fg-meta text-mono-sm">
            {parts.package}
          </span>
        ) : null}
        <Tag variant="soft" tone={release.date_released ? 'brand' : 'neutral'}>
          {release.date_released
            ? tReleases('released')
            : tReleases('unreleased')}
        </Tag>
      </div>
      <div className="flex min-w-0 items-center gap-1">
        <h1 className="truncate font-mono text-fg text-page-title">
          {parts.version}
        </h1>
        <Button
          variant="ghost"
          size="sm"
          icon={CopyIcon}
          aria-label={t('copy')}
          onClick={copy}
        />
      </div>
      <p className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-0.5 text-fg-subtle text-meta">
        <span title={format.dateTime(created, 'dateTime')}>
          {t('created', { when: format.relativeTime(created) })}
        </span>
        {release.date_released ? (
          <span>
            {t('releasedOn', {
              when: format.dateTime(
                new Date(release.date_released),
                'dateTime',
              ),
            })}
          </span>
        ) : null}
        {release.ref ? <Commit release={release} /> : null}
      </p>
    </div>
  );
}

/** The commit it was built from, linked when CI said where it lives. */
function Commit({ release }: { release: ReleaseSummary }) {
  const ref = release.ref ?? '';
  const short = /^[0-9a-f]{12,}$/i.test(ref) ? ref.slice(0, 7) : ref;
  return release.url ? (
    <a
      href={release.url}
      target="_blank"
      rel="noreferrer"
      className="inline-flex items-center gap-1 font-mono hover:text-fg"
    >
      {short}
      <ExternalLinkIcon size="sm" aria-hidden="true" />
    </a>
  ) : (
    <span className="font-mono">{short}</span>
  );
}

// ── chrome ───────────────────────────────────────────────────────────────────

function Panel({
  title,
  description,
  footer,
  children,
  flush,
}: {
  title: string;
  description?: ReactNode;
  footer?: ReactNode;
  children: ReactNode;
  /** Rows that run to the panel's edges. */
  flush?: boolean;
}) {
  return (
    <section className="flex min-w-0 flex-col rounded-2xl border border-border-subtle bg-surface">
      <header className="px-5 pt-4">
        <h2 className="text-card-title text-fg">{title}</h2>
        {description ? (
          <p className="mt-0.5 text-fg-subtle text-meta">{description}</p>
        ) : null}
      </header>
      <div className={cn('min-w-0 flex-1 pt-3 pb-4', flush ? null : 'px-5')}>
        {children}
      </div>
      {footer ? (
        <footer className="border-border-divider border-t px-5 py-3">
          {footer}
        </footer>
      ) : null}
    </section>
  );
}

function Placeholder({ height }: { height: number }) {
  return (
    <div
      className="animate-pulse rounded-lg bg-surface-hover motion-reduce:animate-none"
      style={{ height }}
    />
  );
}

/** The read's data, its placeholder or its failure -- never a fake zero. */
function Loaded<T>({
  read,
  height,
  title,
  inset,
  children,
}: {
  read: Read<T>;
  height: number;
  title: string;
  /** Inside a flush panel, the placeholder and failure keep its margin. */
  inset?: boolean;
  children: (data: T) => ReactNode;
}) {
  const state = (
    <QueryState
      query={read}
      placeholder={<Placeholder height={height} />}
      title={title}
    >
      {children}
    </QueryState>
  );
  return inset && read.data === undefined ? (
    <div className="px-5">{state}</div>
  ) : (
    state
  );
}

// ── KPIs ─────────────────────────────────────────────────────────────────────

function Kpi({
  label,
  value,
  footnote,
}: {
  label: string;
  value: ReactNode;
  footnote?: ReactNode;
}) {
  return (
    <div className="flex min-w-0 flex-col gap-1.5 bg-surface px-4 py-3.5">
      <span className="truncate text-fg-meta text-meta">{label}</span>
      <span className="text-fg text-title tabular-nums">{value}</span>
      <span className="truncate text-fg-subtle text-hint">
        {footnote ?? ' '}
      </span>
    </div>
  );
}

/**
 * The release in six figures. The crash-free rates carry the project's own
 * beside them, as Sentry compares a release to the average: a 97% is fine
 * where everything runs at 96 and alarming where everything runs at 99.9.
 */
export function ReleaseKpis({
  release,
  project,
}: {
  release: ReleaseSummary;
  project: Read<SessionSummary>;
}) {
  const t = useTranslations('releases.detail');
  const format = useFormatter();
  const percent = (rate: number | null | undefined) =>
    rate == null
      ? null
      : format.number(rate, { style: 'percent', maximumFractionDigits: 2 });
  const average = (
    rate: 'crash_free_sessions_rate' | 'crash_free_users_rate',
  ) => {
    // The comparison is a footnote: without it the release's own rate
    // still stands.
    if (!project.data) return null;
    const value = percent(project.data[rate]);
    return value ? t('projectAverage', { rate: value }) : null;
  };
  const none = release.sessions === 0;

  return (
    // One strip, the cells split by hairlines: six cards at two heights
    // would leave a ragged edge for no reason.
    <div className="grid grid-cols-2 gap-px overflow-hidden rounded-2xl border border-border-subtle bg-border-subtle md:grid-cols-3 2xl:grid-cols-6">
      <Kpi
        label={t('crashFreeSessions')}
        value={<HealthRate rate={release.crash_free_sessions} />}
        footnote={none ? t('noSessions') : average('crash_free_sessions_rate')}
      />
      <Kpi
        label={t('crashFreeUsers')}
        value={<HealthRate rate={release.crash_free_users} />}
        footnote={none ? t('noSessions') : average('crash_free_users_rate')}
      />
      <Kpi
        label={t('adoption')}
        value={
          release.adoption == null ? (
            <span className="text-fg-ghost">–</span>
          ) : (
            format.number(release.adoption, {
              style: 'percent',
              maximumFractionDigits: 1,
            })
          )
        }
        footnote={release.adoption == null ? null : t('adoptionHint')}
      />
      <Kpi
        label={t('sessions')}
        value={format.number(release.sessions, 'compact')}
        footnote={
          release.crashed > 0
            ? t('crashedSessions', {
                count: format.number(release.crashed, 'compact'),
              })
            : null
        }
      />
      <Kpi
        label={t('newIssues')}
        value={format.number(release.new_issues)}
        footnote={t('newIssuesHint')}
      />
      <Kpi
        label={t('events')}
        value={format.number(release.event_count, 'compact')}
        footnote={
          release.last_event
            ? t('lastEvent', {
                when: format.relativeTime(new Date(release.last_event)),
              })
            : null
        }
      />
    </div>
  );
}

// ── sessions ─────────────────────────────────────────────────────────────────

/** Its sessions over the window, healthy under crashed. */
export function SessionsPanel({
  read,
  intervalHours,
}: {
  read: Read<SessionsGetTimeseriesResponse>;
  intervalHours: number;
}) {
  const t = useTranslations('releases.detail');
  const format = useFormatter();
  const label = (value: string | number) =>
    format.dateTime(
      new Date(value),
      intervalHours >= 24
        ? { month: 'short', day: 'numeric' }
        : { month: 'short', day: 'numeric', hour: 'numeric' },
    );

  return (
    <Panel title={t('sessionsPanel')} description={t('sessionsPanelHint')}>
      <Loaded read={read} height={240} title={t('sessionsPanel')}>
        {(points) =>
          points.every((p) => p.total === 0) ? (
            <div className="flex h-60 flex-col items-center justify-center gap-1.5 text-center">
              <p className="font-medium text-fg text-value">
                {t('noSessions')}
              </p>
              <p className="max-w-xs text-fg-subtle text-meta">
                {t('noSessionsHint')}
              </p>
            </div>
          ) : (
            <BarsChart
              data={points.map((p) => ({
                t: new Date(p.bucket).getTime(),
                healthy: Math.max(p.total - p.crashed, 0),
                crashed: p.crashed,
              }))}
              series={[
                {
                  key: 'healthy',
                  label: t('healthy'),
                  color: 'var(--chart-3)',
                },
                {
                  key: 'crashed',
                  label: t('crashed'),
                  color: 'var(--sev-error)',
                },
              ]}
              xKey="t"
              height={240}
              formatX={label}
              formatY={(v) => format.number(v, 'compact')}
              label={t('sessionsPanel')}
            />
          )
        }
      </Loaded>
    </Panel>
  );
}

// ── new issues ───────────────────────────────────────────────────────────────

export function NewIssuesPanel({
  projectId,
  read,
  issuesHref,
}: {
  projectId: number;
  read: Read<IssueResponse[]>;
  issuesHref: string;
}) {
  const t = useTranslations('releases.detail');
  const format = useFormatter();

  return (
    <Panel
      flush
      title={t('newIssues')}
      description={t('newIssuesHint')}
      footer={
        <Link
          href={issuesHref}
          className="inline-flex items-center gap-1.5 text-fg-subtle text-meta hover:text-fg"
        >
          {t('viewInIssues')}
          <ArrowRightIcon size="sm" aria-hidden="true" />
        </Link>
      }
    >
      <Loaded inset read={read} height={200} title={t('newIssues')}>
        {(issues) =>
          issues.length === 0 ? (
            <p className="flex h-32 items-center justify-center text-fg-subtle text-meta">
              {t('noNewIssues')}
            </p>
          ) : (
            <ul>
              {issues.map((issue) => (
                <li key={issue.id}>
                  <Link
                    href={`/projects/${projectId}/issues/${issue.id}`}
                    className="flex items-center gap-3 px-5 py-2.5 hover:bg-surface-hover"
                  >
                    <span
                      aria-hidden="true"
                      className={cn(
                        'h-8 w-0.75 shrink-0 rounded-pill',
                        LEVEL_MARK[issue.level ?? ''] ?? 'bg-sev-info',
                      )}
                    />
                    <span className="flex min-w-0 flex-1 flex-col">
                      <span className="truncate font-medium text-fg text-value">
                        {issue.title}
                      </span>
                      <span className="truncate text-fg-subtle text-hint">
                        <span className="font-mono">{issue.short_id}</span>
                        {issue.culprit ? ` · ${issue.culprit}` : null}
                      </span>
                    </span>
                    <span className="flex w-24 shrink-0 flex-col items-end">
                      <span className="font-medium text-fg text-value tabular-nums">
                        {format.number(issue.event_count, 'compact')}
                      </span>
                      <span className="truncate text-fg-meta text-hint">
                        {format.relativeTime(new Date(issue.last_seen))}
                      </span>
                    </span>
                  </Link>
                </li>
              ))}
            </ul>
          )
        }
      </Loaded>
    </Panel>
  );
}

// ── rail ─────────────────────────────────────────────────────────────────────

/** A block of the rail, as on the issue page: a quiet heading, then facts. */
export function RailBlock({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <section className="border-border-divider border-b px-5 py-4 last:border-b-0">
      <h2 className="mb-3 font-mono text-column text-fg-meta uppercase">
        {title}
      </h2>
      {children}
    </section>
  );
}

/** Where it ran, each environment with its own share and health. */
export function ReleaseEnvironments({
  read,
}: {
  read: Read<ReleaseHealthRow[]>;
}) {
  const t = useTranslations('releases.detail');
  const format = useFormatter();

  return (
    <Loaded read={read} height={96} title={t('environments')}>
      {(rows) => {
        const total = rows.reduce((sum, row) => sum + row.total, 0);
        return rows.length === 0 ? (
          <p className="text-fg-subtle text-meta">{t('noSessions')}</p>
        ) : (
          <ul className="flex flex-col gap-3">
            {rows.map((row) => (
              <li key={row.environment} className="flex flex-col gap-1.5">
                <div className="flex items-baseline justify-between gap-3 text-meta">
                  <span className="truncate text-fg">{row.environment}</span>
                  <span className="flex shrink-0 items-baseline gap-3">
                    <span className="text-fg-subtle tabular-nums">
                      {t('sessionsCount', {
                        count: format.number(row.total, 'compact'),
                      })}
                    </span>
                    <HealthRate rate={row.crash_free_sessions_rate} quiet />
                  </span>
                </div>
                <AdoptionBar adoption={total ? row.total / total : null} />
              </li>
            ))}
          </ul>
        );
      }}
    </Loaded>
  );
}

/** The release's facts, one per line, for reading rather than scanning. */
export function ReleaseDetails({ release }: { release: ReleaseSummary }) {
  const t = useTranslations('releases.detail');
  const format = useFormatter();
  const parts = splitVersion(release.version);
  const date = (iso: string | null) =>
    iso ? format.dateTime(new Date(iso), 'dateTime') : null;

  const facts: [string, ReactNode][] = [
    [t('version'), <span className="font-mono">{parts.version}</span>],
    [
      t('package'),
      parts.package && <span className="font-mono">{parts.package}</span>,
    ],
    [t('commit'), release.ref && <Commit release={release} />],
    [t('createdAt'), date(release.date_created)],
    [t('releasedAt'), date(release.date_released) ?? t('notReleased')],
    [t('firstEvent'), date(release.first_event)],
    [t('lastEventAt'), date(release.last_event)],
  ];

  return (
    <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2.5 text-meta">
      {facts
        .filter(([, value]) => value)
        .map(([label, value]) => (
          <div key={label} className="contents">
            <dt className="text-fg-subtle">{label}</dt>
            <dd className="min-w-0 truncate text-right text-fg-secondary">
              {value}
            </dd>
          </div>
        ))}
    </dl>
  );
}
