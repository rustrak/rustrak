import {
  type MessageKey,
  useFormatter,
  usePluginApi,
  usePluginSearch,
  usePluginTranslations,
} from '@rustrak/plugin/ui';
import { Panel, PanelPlaceholder, TimeSeriesChart } from '@rustrak/ui';
import { type EventSeries, filterOf, periodOf, tickFormat } from '../model';

/** Events over the window: the five most common, or one by `name`. */
export function EventsChart({
  name,
  title,
  height = 220,
}: {
  name?: string;
  title: MessageKey;
  /** Lower above a list, so the list keeps the page. */
  height?: number;
}) {
  const t = usePluginTranslations();
  const format = useFormatter();
  const [search] = usePluginSearch();
  const period = periodOf(search);
  const series = usePluginApi<EventSeries>('/event-series', {
    ...filterOf(search),
    name,
  });
  const names = series.data?.names ?? [];
  const rows = (series.data?.rows ?? []).map((r) => ({ ...r, t: r.t * 1000 }));
  return (
    <div className="shrink-0">
      <Panel title={t(title)}>
        {series.isPending ? (
          <PanelPlaceholder height={height} />
        ) : (
          <TimeSeriesChart
            label={t(title)}
            height={height}
            integer
            stacked
            xKey="t"
            data={rows}
            series={names.map((n, i) => ({ key: String(i), label: n }))}
            formatX={(v) => format.dateTime(new Date(v), tickFormat(period))}
          />
        )}
      </Panel>
    </div>
  );
}
