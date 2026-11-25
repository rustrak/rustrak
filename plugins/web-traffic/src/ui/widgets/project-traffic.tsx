import {
  defineWidget,
  PluginLink,
  useFormatter,
  usePluginApi,
  usePluginPermissions,
  usePluginTranslations,
  type WidgetProps,
} from '@rustrak/plugin/ui';
import { Figure, FigureStrip, Panel } from '@rustrak/ui';
import { bounceRate, type Overview } from '../model';

export const config = defineWidget({ zone: 'project.overview.after' });

/** The last day's traffic, at the end of a project's overview. */
/** Only for whoever may read the traffic: the route would refuse the rest. */
export default function ProjectTraffic(
  _: WidgetProps<'project.overview.after'>,
) {
  return usePluginPermissions().can('traffic', 'read') ? <LastDay /> : null;
}

function LastDay() {
  const t = usePluginTranslations();
  const format = useFormatter();
  const day = usePluginApi<Overview>('/overview', { period: '24h' });
  const now = day.data?.current;
  const rate = now ? bounceRate(now) : null;
  return (
    <Panel
      title={t('widget.day')}
      actions={
        <PluginLink to="" className="text-meta text-fg-brand">
          {t('widget.open')}
        </PluginLink>
      }
    >
      <FigureStrip className="shrink-0 grid-cols-3">
        <Figure
          label={t('overview.visitors')}
          value={now ? format.number(now.visitors) : '…'}
        />
        <Figure
          label={t('overview.pageviews')}
          value={now ? format.number(now.pageviews) : '…'}
        />
        <Figure
          label={t('overview.bounce')}
          value={rate === null ? '–' : `${rate}%`}
        />
      </FigureStrip>
    </Panel>
  );
}
