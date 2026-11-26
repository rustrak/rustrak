import {
  definePage,
  type PageProps,
  useFormatter,
  usePluginApi,
  usePluginTrail,
  usePluginTranslations,
} from '@rustrak/plugin/ui';
import { Figure, FigureStrip, Panel, PanelPlaceholder } from '@rustrak/ui';
import { ProfileFacts, VisitSection } from '../../../../components/journey';
import { useRegionName } from '../../../../components/region';
import { TrafficPage } from '../../../../components/traffic-page';
import {
  duration,
  type Profile,
  type Step,
  unshown,
  visits,
} from '../../../../model';

export const config = definePage({ requires: 'visitors:read' });

/** One visitor's day: who they seem to be, and every page and event, by visit. */
export default function VisitorPage({ params }: PageProps<'project'>) {
  const t = usePluginTranslations();
  const format = useFormatter();
  const regionName = useRegionName();
  const id = params.visitorId ?? '';
  usePluginTrail([{ label: id.slice(0, 8), mono: true }]);
  const profile = usePluginApi<Profile>('/visitor', { id });
  const steps = usePluginApi<Step[]>('/journey', { id });
  const p = profile.data;
  const day = visits(steps.data ?? []);
  // A day longer than the journey's list: its visits and steps are a start.
  const missing = p && steps.data ? unshown(steps.data, p) : 0;
  const figure = (
    value: number | undefined,
    show: (n: number) => string = (n) => format.number(n),
  ) => (value === undefined ? '…' : show(value));

  return (
    <TrafficPage
      title={t('journey.title')}
      summary={
        p ? [p.country && regionName(p.country), p.browser, p.os, p.device] : []
      }
      filters={false}
    >
      {profile.isError ? (
        <p className="text-meta text-fg-subtle">{t('journey.gone')}</p>
      ) : null}
      <FigureStrip className="shrink-0 grid-cols-2 md:grid-cols-4">
        <Figure
          label={t('visitor.visits')}
          value={figure(steps.data && day.length, (n) =>
            missing > 0 ? `${format.number(n)}+` : format.number(n),
          )}
        />
        <Figure label={t('overview.pageviews')} value={figure(p?.pageviews)} />
        <Figure label={t('top.events')} value={figure(p?.events)} />
        <Figure
          label={t('overview.duration')}
          value={figure(p && p.last - p.first, duration)}
        />
      </FigureStrip>
      <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_320px]">
        <Panel title={t('journey.steps')} hint={t('journey.privacy')} flush>
          {steps.isPending ? (
            <PanelPlaceholder height={240} />
          ) : (
            <div className="flex flex-col">
              {day.map((visit, n) => (
                <VisitSection key={visit[0]!.at} visit={visit} n={n + 1} />
              ))}
              {missing > 0 ? (
                <p className="px-5 py-3 text-meta text-fg-subtle">
                  {t('journey.partial', { count: missing })}
                </p>
              ) : null}
            </div>
          )}
        </Panel>
        <Panel title={t('visitor.profile')}>
          {p ? <ProfileFacts profile={p} /> : <PanelPlaceholder height={320} />}
        </Panel>
      </div>
    </TrafficPage>
  );
}
