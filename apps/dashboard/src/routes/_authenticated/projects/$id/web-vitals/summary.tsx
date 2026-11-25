import {
  ExternalLinkIcon,
  Page,
  PageHeader,
  PanelPlaceholder,
} from '@rustrak/ui';
import { noop, useQuery } from '@tanstack/react-query';
import { createFileRoute, notFound } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { endpointHref } from '@/features/transaction/model/endpoints';
import { webVitalQueries } from '@/features/web-vital/api/queries';
import { pageSpeedHref } from '@/features/web-vital/model/web-vitals';
import { Verdict } from '@/features/web-vital/ui/components/web-vitals';
import { searchString } from '@/shared/lib/search-params';
import { Link } from '@/shared/ui/components/link';
import { QueryState } from '@/shared/ui/components/query-state';
import { usePageTrail } from '@/shared/ui/hooks/use-page-trail';
import { MetricCards } from './-components/metric-cards';
import {
  apiScope,
  scopeChange,
  validateScope,
  type WebVitalsScope,
} from './-components/scope';
import { ScopeActions } from './-components/web-vitals-filters';

interface PageSearch extends WebVitalsScope {
  name?: string;
}

function reads(projectId: number, search: PageSearch) {
  const scope = { ...apiScope(search), page: search.name };
  return {
    summary: webVitalQueries.summary(projectId, scope),
    // The page's worst loads, for a URL a lab test can open.
    samples: webVitalQueries.samples(projectId, scope),
  };
}

/**
 * One page's web vitals, laid out as the project's are, with ways out to
 * its page loads in Performance and to a lab test in PageSpeed Insights. A
 * row of the Web Vitals list opens it, keeping the filters it was read
 * under.
 */
export const Route = createFileRoute(
  '/_authenticated/projects/$id/web-vitals/summary',
)({
  validateSearch: (search: Record<string, unknown>): PageSearch => ({
    name: searchString(search.name),
    ...validateScope(search),
  }),
  // The name *is* the address: without it there is no page to show.
  beforeLoad: ({ search }) => {
    if (!search.name) throw notFound();
  },
  loaderDeps: ({ search }) => search,
  // Each part lands on its own and draws its own placeholder, so nothing
  // suspends and the page needs no skeleton of its own.
  loader: ({ context: { queryClient }, params, deps }) => {
    const r = reads(params.id, deps);
    void queryClient.query(r.summary).catch(noop);
    void queryClient.query(r.samples).catch(noop);
  },
  component: WebVitalsPageDetail,
});

function WebVitalsPageDetail() {
  const t = useTranslations('webVitals');
  const { id: projectId } = Route.useParams();
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const name = search.name ?? '';
  usePageTrail([{ label: name, mono: true }]);
  const r = reads(projectId, search);
  const summary = useQuery(r.summary);
  const url = useQuery(r.samples).data?.find((sample) => sample.url)?.url;
  const scope: WebVitalsScope = {
    period: search.period,
    environment: search.environment,
    browser: search.browser,
    device: search.device,
  };

  return (
    <div className="flex h-full flex-col max-lg:overflow-y-auto">
      <title>{t('meta.pageTitle', { page: name })}</title>
      <PageHeader
        title={<span className="break-all font-mono">{name}</span>}
        summary={[
          <Link
            key="pageloads"
            href={endpointHref(
              projectId,
              { name, op: 'pageload' },
              { period: search.period, environment: search.environment },
            )}
            className="hover:text-fg"
          >
            {t('pageloadsLink')}
          </Link>,
          ...(url
            ? [
                <a
                  key="pagespeed"
                  href={pageSpeedHref(url)}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 hover:text-fg"
                  title={url}
                >
                  {t('pageSpeedLink')}
                  <ExternalLinkIcon size="sm" />
                </a>,
              ]
            : []),
        ]}
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
        <QueryState
          query={summary}
          placeholder={<PanelPlaceholder height={420} />}
          title={t('score')}
        >
          {(data) => (
            <>
              <Verdict summary={data} />
              <MetricCards
                projectId={projectId}
                summary={data}
                scope={scope}
                page={name}
              />
            </>
          )}
        </QueryState>
      </Page>
    </div>
  );
}
