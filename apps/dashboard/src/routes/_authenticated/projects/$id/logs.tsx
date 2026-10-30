import type { LogResponse, LogsVolumeResponse } from '@rustrak/client';
import {
  Button,
  CloseIcon,
  cn,
  EmptyIcon,
  Page,
  PageHeader,
  Skeleton,
  Text,
} from '@rustrak/ui';
import {
  noop,
  type UseQueryResult,
  useQuery,
  useSuspenseQuery,
} from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { type ReactNode, useDeferredValue } from 'react';
import { useTranslations } from 'use-intl';
import { useShortcuts } from '@/features/issue/ui/components/issue-actions';
import { logQueries } from '@/features/log/api/queries';
import { LOG_LEVELS } from '@/features/log/model/log-view';
import { LogDetail } from '@/features/log/ui/components/log-detail';
import { LogList } from '@/features/log/ui/components/log-list';
import {
  LogSearch,
  type LogSearchValue,
} from '@/features/log/ui/components/log-search';
import { LogVolume } from '@/features/log/ui/components/log-volume';
import { projectQueries } from '@/features/project/api/queries';
import {
  DEFAULT_PERIOD,
  PERIOD_CHOICES,
  type PeriodChoice,
} from '@/features/release/model/session-health';
import { PeriodFilter } from '@/features/release/ui/components/period-filter';
import type { ApiError } from '@/shared/api/query';
import {
  searchOneOf,
  searchPage,
  searchString,
} from '@/shared/lib/search-params';
import { PageLoading } from '@/shared/ui/components/page-loading';

const PAGE_SIZE = 50;

interface LogsReadSearch {
  q?: string;
  level?: (typeof LOG_LEVELS)[number];
  trace?: string;
  period?: PeriodChoice;
  page?: number;
}

/**
 * The page's reads for a URL, built once for the loader and the page so both
 * name the same cache entries.
 */
function logReads(projectId: number, search: LogsReadSearch) {
  const period = search.period ?? DEFAULT_PERIOD;
  const filters = {
    query: search.q,
    level: search.level,
    trace_id: search.trace,
    period: period === 'all' ? undefined : period,
  };
  return {
    list: logQueries.list(projectId, {
      ...filters,
      page: search.page ?? 1,
      per_page: PAGE_SIZE,
    }),
    volume: logQueries.volume(projectId, filters),
  };
}

/** A new search starts over at the newest line, with nothing open. */
const narrowed =
  (next: LogSearchValue & { period?: PeriodChoice }) =>
  <S extends object>(prev: S) => ({
    ...prev,
    ...next,
    page: undefined,
    selected: undefined,
  });

/**
 * Logs: the lines the SDKs send, newest first.
 *
 * What every log explorer converged on, at the size of this one: a search
 * bar that takes free text for the message and `level:` and `trace:` keys,
 * the window beside it, the volume stacked by level above the lines, and a
 * line opened in a pane beside the list -- j and k to move, Esc to close.
 * Everything is in the URL, so a line can be linked to.
 */
export const Route = createFileRoute('/_authenticated/projects/$id/logs')({
  validateSearch: (search: Record<string, unknown>) => ({
    q: searchString(search.q),
    level: searchOneOf(search.level, LOG_LEVELS),
    trace: searchString(search.trace),
    period: searchOneOf(search.period, PERIOD_CHOICES),
    page: searchPage(search.page),
    selected: searchString(search.selected),
  }),
  loaderDeps: ({ search }) => ({
    q: search.q,
    level: search.level,
    trace: search.trace,
    period: search.period,
    page: search.page,
  }),
  // The page waits for the project and the lines. The chart is read on its
  // own: a volume that fails leaves the lines readable.
  loader: ({ context: { queryClient }, params, deps }) => {
    const projectId = Number.parseInt(params.id, 10);
    const reads = logReads(projectId, deps);
    void queryClient.query(projectQueries.detail(projectId)).catch(noop);
    void queryClient.query(reads.list).catch(noop);
    void queryClient.query(reads.volume).catch(noop);
  },
  pendingComponent: LogsLoading,
  component: LogsPage,
});

/**
 * The lines follow the address a beat behind: a new search or page keeps the
 * current lines on screen, dimmed, until the next ones are in. What is
 * deferred is the loader's deps, not the whole search, so opening a line
 * (`selected`) never dims the list. The controls and the open line read the
 * live address.
 */
function LogsPage() {
  const t = useTranslations('projectPages');
  const tLogs = useTranslations('logs');
  const live = Route.useSearch();
  // The router keeps the deps object while they are equal, so the two only
  // differ while a new read is on its way.
  const liveDeps = Route.useLoaderDeps();
  const search = useDeferredValue(liveDeps);
  const navigate = Route.useNavigate();
  const isPending = search !== liveDeps;
  const { id } = Route.useParams();
  const projectId = Number.parseInt(id, 10);
  const period = search.period ?? DEFAULT_PERIOD;
  const page = search.page ?? 1;
  const reads = logReads(projectId, search);
  const { data: project } = useSuspenseQuery(projectQueries.detail(projectId));
  const { data: logs } = useSuspenseQuery(reads.list);
  const volume = useQuery(reads.volume);

  const items = logs.items;
  const selection = useSelectedLog(projectId, items, live.selected);
  const current = selection.log;

  const select = (selected: string | undefined) =>
    navigate({ search: (prev) => ({ ...prev, selected }), replace: true });

  const step = (by: number) => {
    const at = current ? items.indexOf(current) + by : 0;
    const next = items[at];
    if (next) select(next.id);
  };

  useShortcuts({
    j: () => step(1),
    k: () => step(-1),
    Escape: () => select(undefined),
  });

  const filtered = Boolean(search.q || search.level || search.trace);
  const howToSend = t.rich('logs.emptyDescription', {
    code: (chunks) => <code className="font-mono">{chunks}</code>,
  });

  return (
    <div className="flex h-full flex-col">
      <title>{t('logs.meta.title', { project: project.name })}</title>
      <PageHeader
        title={t('logs.title')}
        summary={[project.name, tLogs('count', { count: logs.total_count })]}
      />

      {logs.total_count === 0 && !filtered && period === 'all' ? (
        <NoLogsYet title={t('logs.emptyTitle')} hint={howToSend} />
      ) : (
        <Page scroll={false}>
          <LogsFilters lineOpen={Boolean(current)} />

          <VolumePanel volume={volume} lineOpen={Boolean(current)} />

          <LineFailed
            show={selection.failed}
            onClose={() => select(undefined)}
          />

          <div className="flex min-h-0 flex-1 gap-4">
            {/* Narrow, the pane replaces the list once a line is open. */}
            <div
              className={cn(
                'min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-lg border border-border bg-panel lg:flex',
                current ? 'hidden' : 'flex',
              )}
            >
              <LogList
                logs={logs}
                search={search.q ?? ''}
                selectedId={current?.id}
                // Nothing narrows it, so the window holds no logs: say how to
                // send some, in words that fit "none lately" as well as "none".
                emptyHint={filtered ? undefined : howToSend}
                page={page}
                isPending={isPending}
                onSelect={(id) => select(id === current?.id ? undefined : id)}
                onPageChange={(next) =>
                  navigate({
                    search: (prev) => ({
                      ...prev,
                      page: next === 1 ? undefined : next,
                      selected: undefined,
                    }),
                  })
                }
              />
            </div>
            <SelectedPane
              selection={selection}
              onTrace={(trace) => navigate({ search: narrowed({ trace }) })}
              onClose={() => select(undefined)}
            />
          </div>
        </Page>
      )}
    </div>
  );
}

/**
 * The same page while the project and the lines load: the search and the
 * window already work, the chart fills in on its own, and the lines are
 * outlines.
 */
function LogsLoading() {
  const t = useTranslations('projectPages');
  const { id } = Route.useParams();
  const search = Route.useSearch();
  const volume = useQuery(logReads(Number.parseInt(id, 10), search).volume);

  return (
    <PageLoading>
      <div className="flex h-full flex-col">
        <PageHeader
          title={t('logs.title')}
          summary={[<Skeleton key="summary" className="h-4 w-40" />]}
        />
        <Page scroll={false}>
          <LogsFilters lineOpen={false} />
          <VolumePanel volume={volume} lineOpen={false} />
          <ul className="min-h-0 flex-1 divide-y divide-border-divider overflow-hidden rounded-lg border border-border bg-panel">
            {Array.from({ length: 12 }, (_, line) => (
              // Placeholder lines have no identity beyond their position.
              // react-doctor-disable-next-line react-doctor/no-array-index-as-key
              <li key={line} className="flex items-center gap-3 py-2 pr-4 pl-2">
                <Skeleton className="h-4 w-0.75 rounded-pill" />
                <Skeleton className="h-3 w-10" />
                <Skeleton className="h-3 w-24" />
                <Skeleton className="h-3 flex-1" />
              </li>
            ))}
          </ul>
        </Page>
      </div>
    </PageLoading>
  );
}

/** The search bar and the window, reading and writing the live address. */
function LogsFilters({ lineOpen }: { lineOpen: boolean }) {
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const narrow = (next: LogSearchValue & { period?: PeriodChoice }) =>
    navigate({ search: narrowed(next) });

  return (
    // Narrow, the bar takes a row and the window the next; with a line open,
    // the pane takes the whole page.
    <div
      className={cn(
        'flex flex-wrap items-center gap-2',
        lineOpen && 'max-lg:hidden',
      )}
    >
      <LogSearch
        value={{ q: search.q, level: search.level, trace: search.trace }}
        onChange={narrow}
      />
      <PeriodFilter
        period={search.period ?? DEFAULT_PERIOD}
        onChange={(next) =>
          narrow({ period: next === DEFAULT_PERIOD ? undefined : next })
        }
      />
    </div>
  );
}

/** A project that has never sent a log: how to send the first. */
function NoLogsYet({ title, hint }: { title: string; hint: ReactNode }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 px-page-gutter py-16 text-center">
      <EmptyIcon size="xl" className="mb-2 text-fg-ghost" />
      <Text variant="body">{title}</Text>
      <Text variant="meta" tone="subtle" className="max-w-md">
        {hint}
      </Text>
    </div>
  );
}

/**
 * The volume above the lines, or a line saying it failed: the chart failing
 * never takes the lines with it. On a phone, and beside an open line, the
 * lines need the height more than the chart does.
 */
function VolumePanel({
  volume,
  lineOpen,
}: {
  volume: UseQueryResult<LogsVolumeResponse, ApiError>;
  lineOpen: boolean;
}) {
  const tLogs = useTranslations('logs');
  if (volume.data?.length === 0) return null;
  return (
    <div
      className={cn(
        'shrink-0 rounded-lg border border-border bg-panel px-2 pt-3 pb-1 max-sm:hidden',
        lineOpen && 'max-lg:hidden',
      )}
    >
      {volume.data ? (
        <LogVolume points={volume.data} />
      ) : volume.isError ? (
        <Text variant="meta" tone="subtle" className="px-2 pb-2">
          {tLogs('volumeFailed')}
        </Text>
      ) : (
        // The chart's height while it loads, so the lines do not jump.
        <div className="h-[120px]" />
      )}
    </div>
  );
}

/** The open line, or that the one the address names could not be read. */
interface Selection {
  log: LogResponse | undefined;
  failed: boolean;
}

/**
 * The line `?selected=` names: from the page when it is on it, fetched on its
 * own when newer lines have pushed it off, so a linked line still opens.
 */
function useSelectedLog(
  projectId: number,
  items: LogResponse[],
  selected: string | undefined,
): Selection {
  const onPage = items.find((log) => log.id === selected);
  const fetched = useQuery({
    ...logQueries.detail(projectId, selected ?? ''),
    enabled: selected !== undefined && !onPage,
  });
  if (onPage) return { log: onPage, failed: false };
  if (selected === undefined) return { log: undefined, failed: false };
  return { log: fetched.data, failed: fetched.isError };
}

/** Beside the list, or in its place on a phone: the open line. */
function SelectedPane({
  selection,
  onTrace,
  onClose,
}: {
  selection: Selection;
  onTrace: (trace: string) => void;
  onClose: () => void;
}) {
  if (!selection.log) return null;
  return (
    <div className="flex min-h-0 w-full flex-col overflow-hidden lg:w-[28rem] lg:flex-none lg:rounded-lg lg:border lg:border-border lg:bg-panel xl:w-[34rem]">
      <LogDetail
        key={selection.log.id}
        log={selection.log}
        onTrace={onTrace}
        onClose={onClose}
      />
    </div>
  );
}

/**
 * A linked line that could not be read, said above the list rather than in
 * the pane: the list stays whole on a phone, and closing it clears the link.
 */
function LineFailed({ show, onClose }: { show: boolean; onClose: () => void }) {
  const tLogs = useTranslations('logs');
  if (!show) return null;
  return (
    <div className="flex shrink-0 items-center justify-between gap-2 rounded-lg border border-border bg-panel py-1 pr-1 pl-3">
      <Text variant="meta" tone="subtle">
        {tLogs('lineFailed')}
      </Text>
      <Button
        variant="ghost"
        size="sm"
        icon={CloseIcon}
        aria-label={tLogs('close')}
        onClick={onClose}
      />
    </div>
  );
}
