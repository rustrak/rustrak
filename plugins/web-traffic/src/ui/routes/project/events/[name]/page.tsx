import {
  type PageProps,
  PluginLink,
  useFormatter,
  usePluginApi,
  usePluginPermissions,
  usePluginSearch,
  usePluginTrail,
  usePluginTranslations,
} from '@rustrak/plugin/ui';
import { Figure, FigureStrip, Panel, PanelPlaceholder } from '@rustrak/ui';
import { BAR_ROW, BarFill } from '../../../../components/bar';
import { EventsChart } from '../../../../components/events-chart';
import { TrafficPage } from '../../../../components/traffic-page';
import {
  byKey,
  type EventTotals,
  filterOf,
  type Occurrence,
  type Property,
  periodOf,
  type Row,
  share,
} from '../../../../model';

/** A bar list: each value with its share of the largest. */
function Bars({ rows }: { rows: { value: string; count: number }[] }) {
  const format = useFormatter();
  const max = Math.max(1, ...rows.map((r) => r.count));
  const total = rows.reduce((sum, r) => sum + r.count, 0);
  return (
    <div className="flex flex-col py-2">
      {rows.map((r) => (
        <div key={r.value} className={BAR_ROW}>
          <BarFill ratio={r.count / max} />
          <span className="relative min-w-0 flex-1 truncate text-fg-secondary">
            {r.value}
          </span>
          <span className="relative tabular-nums text-fg-ghost">
            {share(r.count, total)}%
          </span>
          <span className="relative w-10 text-right tabular-nums text-fg-subtle">
            {format.number(r.count)}
          </span>
        </div>
      ))}
    </div>
  );
}

/** One event: how often, by whom, where, over time, and what it carried. */
export default function EventDetail({ params }: PageProps<'project'>) {
  const t = usePluginTranslations();
  const format = useFormatter();
  const [search] = usePluginSearch();
  const filter = filterOf(search);
  // Who did it, one by one, is for whoever may see visitors.
  const canSeeVisitors = usePluginPermissions().can('visitors', 'read');
  const name = params.name ?? '';
  usePluginTrail([{ label: name }]);
  const totals = usePluginApi<EventTotals>('/event', { name, ...filter });
  const properties = usePluginApi<Property[]>('/event-properties', {
    name,
    ...filter,
  });
  const pages = usePluginApi<Row[]>('/event-pages', { name, ...filter });
  const row = totals.data;
  const conversion = row ? share(row.visitors, row.everyone) : null;
  const shown = (value?: number) =>
    value === undefined ? (totals.isPending ? '…' : '0') : format.number(value);

  return (
    <TrafficPage
      title={name}
      summary={[t('event.kind'), t(`period.${periodOf(search)}`)]}
    >
      <FigureStrip className="shrink-0 grid-cols-2 md:grid-cols-4">
        <Figure label={t('top.count')} value={shown(row?.count)} />
        <Figure label={t('overview.visitors')} value={shown(row?.visitors)} />
        <Figure
          label={t('event.conversion')}
          value={conversion === null ? '–' : `${conversion}%`}
          footnote={t('event.conversionHint')}
        />
        <Figure
          label={t('event.perVisitor')}
          value={
            row && row.visitors > 0
              ? format.number(row.count / row.visitors, {
                  maximumFractionDigits: 1,
                })
              : '–'
          }
        />
      </FigureStrip>
      <EventsChart name={name} title="events.overTime" />
      <div className="grid gap-4 md:grid-cols-2">
        <Panel title={t('event.pages')} flush>
          {pages.isPending ? (
            <PanelPlaceholder height={120} />
          ) : (
            <Bars
              rows={(pages.data ?? []).map((p) => ({
                value: p.value,
                count: p.count,
              }))}
            />
          )}
        </Panel>
        {byKey(properties.data ?? []).map(([key, values]) => (
          <Panel key={key} title={key} hint={t('event.property')} flush>
            <Bars
              rows={values.map((v) => ({ value: v.value, count: v.count }))}
            />
          </Panel>
        ))}
      </div>
      {canSeeVisitors ? <RecentOccurrences name={name} /> : null}
    </TrafficPage>
  );
}

/** The last times the event happened, each with who did it. */
function RecentOccurrences({ name }: { name: string }) {
  const t = usePluginTranslations();
  const format = useFormatter();
  const [search] = usePluginSearch();
  const recent = usePluginApi<Occurrence[]>('/event-recent', {
    name,
    ...filterOf(search),
  });
  return (
    <Panel title={t('event.recent')} flush>
      {recent.isPending ? (
        <PanelPlaceholder height={160} />
      ) : (
        <ul className="flex flex-col py-2">
          {(recent.data ?? []).map((o, i) => (
            // Occurrences have no id; time and order tell them apart.
            // react-doctor-disable-next-line react-doctor/no-array-index-as-key
            <li
              key={i}
              className="flex items-baseline gap-4 px-5 py-1.5 text-meta"
            >
              <span className="w-32 shrink-0 tabular-nums text-fg-subtle">
                {format.dateTime(new Date(o.at * 1000), {
                  day: 'numeric',
                  month: 'short',
                  hour: '2-digit',
                  minute: '2-digit',
                })}
              </span>
              <span className="min-w-0 flex-1 truncate text-fg-secondary">
                {o.path}
              </span>
              {o.data ? (
                <span className="hidden truncate font-mono text-fg-subtle md:inline">
                  {o.data}
                </span>
              ) : null}
              <PluginLink
                to={`visitors/${o.visitor}`}
                className="shrink-0 font-mono text-fg-brand"
              >
                {o.visitor.slice(0, 8)}
              </PluginLink>
            </li>
          ))}
        </ul>
      )}
    </Panel>
  );
}
