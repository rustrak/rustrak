import {
  definePage,
  HostLink,
  type PageProps,
  setupHref,
  useFormatter,
  usePlugin,
  usePluginApi,
  usePluginSearch,
  usePluginTranslations,
} from '@rustrak/plugin/ui';
import { Figure, FigureStrip, Panel, TimeSeriesChart } from '@rustrak/ui';
import { useRegionName } from '../../components/region';
import { TopList } from '../../components/top-list';
import { TrafficPage } from '../../components/traffic-page';
import {
  bounceRate,
  change,
  duration,
  filterOf,
  type Overview,
  type Period,
  periodOf,
  tickFormat,
  visitLength,
} from '../../model';

export const config = definePage({
  sidebar: { label: 'nav.traffic', icon: 'overview', rank: 1 },
});

/** A project's web traffic: who came, from where, to what. */
export default function Traffic(_: PageProps<'project'>) {
  const t = usePluginTranslations();
  const plugin = usePlugin();
  const format = useFormatter();
  const [search] = usePluginSearch();
  const filter = filterOf(search);
  const overview = usePluginApi<Overview>('/overview', filter);
  const live = usePluginApi<{ visitors: number }>('/live');
  const regionName = useRegionName();

  const current = overview.data?.current;
  const previous = overview.data?.previous;
  const delta = (now?: number, before?: number) => {
    const pct =
      now === undefined || before === undefined ? null : change(now, before);
    return pct === null ? undefined : t('overview.change', { pct });
  };
  const rate = current ? bounceRate(current) : null;
  const length = current ? visitLength(current) : null;
  const empty = current?.pageviews === 0 && previous?.pageviews === 0;

  return (
    <TrafficPage
      title={t('overview.title')}
      summary={[
        live.data ? t('overview.online', { count: live.data.visitors }) : null,
        t(`period.${periodOf(search)}`),
      ]}
    >
      {empty && Object.keys(search).length === 0 ? (
        <p className="text-meta text-fg-subtle">
          {t('overview.empty')}{' '}
          <HostLink href={setupHref(plugin)} className="text-fg-brand">
            {t('nav.setup')}
          </HostLink>
        </p>
      ) : null}
      <FigureStrip className="shrink-0 grid-cols-2 md:grid-cols-4">
        <Figure
          label={t('overview.visitors')}
          value={current ? format.number(current.visitors) : '…'}
          footnote={delta(current?.visitors, previous?.visitors)}
        />
        <Figure
          label={t('overview.pageviews')}
          value={current ? format.number(current.pageviews) : '…'}
          footnote={delta(current?.pageviews, previous?.pageviews)}
        />
        <Figure
          label={t('overview.bounce')}
          value={rate === null ? '–' : `${rate}%`}
        />
        <Figure
          label={t('overview.duration')}
          value={length === null ? '–' : duration(length)}
        />
      </FigureStrip>
      <Panel title={t('overview.chart')}>
        <TimeSeriesChart
          label={t('overview.chart')}
          height={220}
          integer
          xKey="t"
          data={(overview.data?.series ?? []).map((b) => ({
            ...b,
            t: b.t * 1000,
          }))}
          series={[
            { key: 'visitors', label: t('overview.visitors') },
            { key: 'pageviews', label: t('overview.pageviews') },
          ]}
          formatX={(v) =>
            format.dateTime(new Date(v), tickFormat(filter.period as Period))
          }
        />
      </Panel>
      <div className="grid gap-4 md:grid-cols-2">
        <TopList
          title="top.pages"
          rankings={['path', 'entry', 'exit', 'title']}
        />
        <TopList
          title="top.sources"
          rankings={['referrer', 'source', 'medium', 'campaign']}
        />
        <TopList
          title="top.location"
          rankings={['country', 'language']}
          label={(by, value) => (by === 'country' ? regionName(value) : value)}
        />
        <TopList
          title="top.environment"
          rankings={['browser', 'os', 'device']}
        />
        <TopList title="top.events" rankings={['event']} />
      </div>
    </TrafficPage>
  );
}
