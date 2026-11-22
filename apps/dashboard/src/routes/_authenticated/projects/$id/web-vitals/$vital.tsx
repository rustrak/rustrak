import type { Vital } from '@rustrak/client';
import { Page, PageHeader } from '@rustrak/ui';
import { noop, useQuery } from '@tanstack/react-query';
import { createFileRoute, notFound } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { webVitalQueries } from '@/features/web-vital/api/queries';
import {
  pageHref,
  VITAL_NAMES,
  VITALS,
} from '@/features/web-vital/model/web-vitals';
import {
  BreakdownTable,
  CausesList,
  Guidance,
  PercentileList,
  RatingTrendChart,
  SamplesList,
  VitalReading,
  WorstPages,
} from '@/features/web-vital/ui/components/web-vitals';
import { searchString } from '@/shared/lib/search-params';
import { Panel, PanelPlaceholder } from '@/shared/ui/components/panel';
import { QueryState } from '@/shared/ui/components/query-state';
import { usePageTrail } from '@/shared/ui/hooks/use-page-trail';
import {
  apiScope,
  scopeChange,
  validateScope,
  type WebVitalsScope,
} from './-components/scope';
import { ScopeActions } from './-components/web-vitals-filters';

interface MetricSearch extends WebVitalsScope {
  /** One page's, when opened from it. */
  page?: string;
}

/** How many pages the vital's page lists. */
const WORST_PAGES = 10;

/** Every read of one vital's page, for the loader and the panels alike. */
function reads(projectId: number, vital: Vital, search: MetricSearch) {
  const scope = { ...apiScope(search), page: search.page };
  const one = { ...scope, vital };
  return {
    summary: webVitalQueries.summary(projectId, scope),
    targets: webVitalQueries.targets(projectId, one),
    devices: webVitalQueries.breakdown(projectId, { ...one, by: 'device' }),
    browsers: webVitalQueries.breakdown(projectId, { ...one, by: 'browser' }),
    trend: webVitalQueries.trend(projectId, one),
    samples: webVitalQueries.samples(projectId, one),
    // Across the project only: on one page there is one page.
    pages: search.page
      ? undefined
      : webVitalQueries.pages(projectId, {
          ...apiScope(search),
          vital,
          per_page: WORST_PAGES,
        }),
  };
}

/**
 * One vital, everything behind it on one page: how it reads and spreads,
 * what usually fixes it, the elements that cause it, where it is worst,
 * how it moved and its worst page loads. A Web Vitals card opens it, for
 * the project or for the page it was on.
 */
export const Route = createFileRoute(
  '/_authenticated/projects/$id/web-vitals/$vital',
)({
  params: {
    parse: ({ vital }) => {
      if (!(VITALS as string[]).includes(vital)) throw notFound();
      return { vital: vital as Vital };
    },
    stringify: ({ vital }) => ({ vital }),
  },
  validateSearch: (search: Record<string, unknown>): MetricSearch => ({
    page: searchString(search.page),
    ...validateScope(search),
  }),
  loaderDeps: ({ search }) => search,
  // Each panel lands on its own and draws its own placeholder, so nothing
  // suspends and the page needs no skeleton of its own.
  loader: ({ context: { queryClient }, params, deps }) => {
    const r = reads(params.id, params.vital, deps);
    void queryClient.query(r.summary).catch(noop);
    void queryClient.query(r.targets).catch(noop);
    void queryClient.query(r.devices).catch(noop);
    void queryClient.query(r.browsers).catch(noop);
    void queryClient.query(r.trend).catch(noop);
    void queryClient.query(r.samples).catch(noop);
    if (r.pages) void queryClient.query(r.pages).catch(noop);
  },
  component: MetricPage,
});

function MetricPage() {
  const t = useTranslations('webVitals');
  const tDetail = useTranslations('webVitals.detail');
  const tDevices = useTranslations('webVitals.devices');
  const tVital = useTranslations('webVitals.vitals');
  const { id: projectId, vital } = Route.useParams();
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const scope: WebVitalsScope = {
    period: search.period,
    environment: search.environment,
    browser: search.browser,
    device: search.device,
  };
  const name = VITAL_NAMES[vital];
  usePageTrail([
    ...(search.page
      ? [
          {
            label: search.page,
            mono: true,
            href: pageHref(projectId, search.page, scope),
          },
        ]
      : []),
    { label: name.short },
  ]);
  const r = reads(projectId, vital, search);
  const summary = useQuery(r.summary);
  const targets = useQuery(r.targets);
  const devices = useQuery(r.devices);
  const browsers = useQuery(r.browsers);
  const trend = useQuery(r.trend);
  const stats = summary.data?.vitals.find((v) => v.vital === vital);
  const placeholder = <PanelPlaceholder height={200} />;

  return (
    <div className="flex h-full flex-col max-lg:overflow-y-auto">
      <title>
        {t('meta.metricTitle', {
          vital: search.page ? `${name.short} · ${search.page}` : name.short,
        })}
      </title>
      <PageHeader
        title={
          <span className="flex min-w-0 items-baseline gap-3">
            {name.short}
            <span className="truncate text-fg-subtle text-section">
              {name.long}
            </span>
          </span>
        }
        summary={
          search.page
            ? [
                <span key="page" className="font-mono">
                  {search.page}
                </span>,
              ]
            : [t('allPages')]
        }
        actions={
          <ScopeActions
            projectId={projectId}
            scope={scope}
            browsers={summary.data?.browsers ?? []}
            onChange={(next) =>
              navigate({
                search: (prev) => ({ ...prev, ...scopeChange(next) }),
                replace: true,
              })
            }
          />
        }
      />
      <Page className="max-lg:flex-none max-lg:overflow-visible">
        <p className="max-w-3xl shrink-0 text-fg-secondary text-meta">
          {tVital(vital)}
        </p>

        <div className="grid shrink-0 gap-3 lg:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]">
          <Panel title={tDetail('reading')} hint={tDetail('readingHint')}>
            <QueryState
              query={summary}
              placeholder={placeholder}
              title={name.short}
            >
              {() =>
                stats ? (
                  <div className="flex flex-col gap-4">
                    <VitalReading vital={vital} stats={stats} />
                    <PercentileList vital={vital} stats={stats} />
                    <p className="text-fg-subtle text-hint">
                      {tDetail('measured', { count: stats.count })}
                    </p>
                  </div>
                ) : (
                  <p className="text-fg-subtle text-meta">{t('noDataFor')}</p>
                )
              }
            </QueryState>
          </Panel>
          <Panel title={tDetail('howToFix')} hint={tDetail('howToFixHint')}>
            <Guidance vital={vital} />
          </Panel>
        </div>

        <div className="grid shrink-0 gap-3 lg:grid-cols-2">
          <Panel flush title={tDetail('causes')} hint={tDetail('causesHint')}>
            <QueryState
              query={targets}
              placeholder={placeholder}
              title={tDetail('causes')}
            >
              {(rows) => <CausesList vital={vital} targets={rows} />}
            </QueryState>
          </Panel>
          {r.pages ? (
            <WorstPagesPanel
              projectId={projectId}
              vital={vital}
              scope={scope}
              read={r.pages}
            />
          ) : (
            <Panel
              flush
              title={tDetail('samples')}
              hint={tDetail('samplesHint')}
            >
              <SamplesPanelBody
                projectId={projectId}
                vital={vital}
                read={r.samples}
              />
            </Panel>
          )}
          <Panel flush title={tDevices('title')} hint={tDevices('hint')}>
            <QueryState
              query={devices}
              placeholder={placeholder}
              title={tDevices('title')}
            >
              {(rows) => (
                <BreakdownTable
                  vital={vital}
                  rows={rows}
                  label={(key) =>
                    key === 'mobile' || key === 'desktop'
                      ? tDevices(key)
                      : tDevices('unknown')
                  }
                />
              )}
            </QueryState>
          </Panel>
          <Panel
            flush
            title={tDetail('browsers')}
            hint={tDetail('browsersHint')}
          >
            <QueryState
              query={browsers}
              placeholder={placeholder}
              title={tDetail('browsers')}
            >
              {(rows) => (
                <BreakdownTable
                  vital={vital}
                  rows={rows}
                  label={(key) => key ?? tDevices('unknown')}
                />
              )}
            </QueryState>
          </Panel>
        </div>

        <Panel
          title={t('trendOf', { vital: name.short })}
          hint={t('trendHint')}
        >
          <QueryState
            query={trend}
            placeholder={placeholder}
            title={t('trend')}
          >
            {(points) => <RatingTrendChart vital={vital} points={points} />}
          </QueryState>
        </Panel>

        {r.pages ? (
          <Panel flush title={tDetail('samples')} hint={tDetail('samplesHint')}>
            <SamplesPanelBody
              projectId={projectId}
              vital={vital}
              read={r.samples}
            />
          </Panel>
        ) : null}
      </Page>
    </div>
  );
}

/** The pages the vital is worst on, each opening that page's own. */
function WorstPagesPanel({
  projectId,
  vital,
  scope,
  read,
}: {
  projectId: number;
  vital: Vital;
  scope: WebVitalsScope;
  read: ReturnType<typeof webVitalQueries.pages>;
}) {
  const tDetail = useTranslations('webVitals.detail');
  const pages = useQuery(read);
  return (
    <Panel flush title={tDetail('pages')} hint={tDetail('pagesHint')}>
      <QueryState
        query={pages}
        placeholder={<PanelPlaceholder height={200} />}
        title={tDetail('pages')}
      >
        {(rows) => (
          <WorstPages
            vital={vital}
            pages={rows.items}
            href={(page) => pageHref(projectId, page, scope)}
          />
        )}
      </QueryState>
    </Panel>
  );
}

/** The vital's worst measurements, each opening its page load's trace. */
function SamplesPanelBody({
  projectId,
  vital,
  read,
}: {
  projectId: number;
  vital: Vital;
  read: ReturnType<typeof webVitalQueries.samples>;
}) {
  const tDetail = useTranslations('webVitals.detail');
  const samples = useQuery(read);
  return (
    <QueryState
      query={samples}
      placeholder={<PanelPlaceholder height={200} />}
      title={tDetail('samples')}
    >
      {(rows) => (
        <SamplesList
          vital={vital}
          samples={rows}
          transactionHref={(id) => `/projects/${projectId}/performance/${id}`}
        />
      )}
    </QueryState>
  );
}
