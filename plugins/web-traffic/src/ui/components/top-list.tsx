import {
  type MessageKey,
  PluginLink,
  useFormatter,
  usePluginApi,
  usePluginSearch,
  usePluginTranslations,
} from '@rustrak/plugin/ui';
import { Panel, PanelPlaceholder, Tab, TabList, Tabs } from '@rustrak/ui';
import { useState } from 'react';
import { eventPage, filterOf, narrows, type Ranking, type Row } from '../model';
import { BAR_ROW, BarFill } from './bar';
import { LoadFailed } from './load-failed';

interface TopListProps {
  title: MessageKey;
  /** What it can rank, as tabs when there are several. */
  rankings: readonly Ranking[];
  /** How a value reads: a country code as its name, say. */
  label?: (by: Ranking, value: string) => string;
}

/**
 * The most common values of one dimension, each with a bar of its share.
 * A value narrows the whole report to it; "All" opens the full list.
 */
export function TopList({ title, rankings, label }: TopListProps) {
  const t = usePluginTranslations();
  const [picked, setBy] = useState<Ranking>();
  const by = picked ?? rankings[0]!;
  const [search] = usePluginSearch();
  return (
    <Panel
      title={t(title)}
      flush
      actions={
        rankings.length > 1 ? (
          <Tabs value={by} onValueChange={(next) => setBy(next as Ranking)}>
            <TabList>
              {rankings.map((r) => (
                <Tab key={r} value={r} size="sm">
                  {t(`ranking.${r}`)}
                </Tab>
              ))}
            </TabList>
          </Tabs>
        ) : null
      }
      footer={
        <PluginLink
          to={`breakdown/${by}`}
          search={search}
          className="text-meta text-fg-brand"
        >
          {t('top.all')}
        </PluginLink>
      }
    >
      <Rows by={by} label={label} />
    </Panel>
  );
}

function Rows({ by, label }: { by: Ranking; label?: TopListProps['label'] }) {
  const t = usePluginTranslations();
  const format = useFormatter();
  const [search, setSearch] = usePluginSearch();
  const rows = usePluginApi<Row[]>('/top', { by, ...filterOf(search) });
  // Events are counted; everything else by the visitors behind it.
  const measure = (r: Row) => (by === 'event' ? r.count : r.visitors);
  const max = Math.max(1, ...(rows.data ?? []).map(measure));
  const dimension = narrows(by);

  if (rows.isPending) return <PanelPlaceholder height={176} />;
  if (!rows.data) return <LoadFailed retry={rows.refetch} />;
  if (rows.data.length === 0) {
    return (
      <p className="px-5 py-6 text-meta text-fg-subtle">{t('top.none')}</p>
    );
  }
  return (
    <div className="flex flex-col py-2">
      <div className="flex justify-between px-5 pb-1 text-meta text-fg-ghost">
        <span>{t(`ranking.${by}`)}</span>
        <span>{t(by === 'event' ? 'top.count' : 'top.visitors')}</span>
      </div>
      {(rows.data ?? []).map((row) => {
        const shown = label ? label(by, row.value) : row.value;
        const content = (
          <>
            <BarFill ratio={measure(row) / max} />
            <span className="relative min-w-0 flex-1 truncate text-left text-fg-secondary">
              {shown}
            </span>
            <span className="relative tabular-nums text-fg-subtle">
              {format.number(measure(row))}
            </span>
          </>
        );
        if (by === 'event') {
          return (
            <PluginLink
              key={row.value}
              to={eventPage(row.value)}
              search={search}
              className={`${BAR_ROW} hover:bg-surface-hover`}
            >
              {content}
            </PluginLink>
          );
        }
        return (
          <button
            key={row.value}
            type="button"
            title={t('top.narrow', { value: shown })}
            onClick={() => dimension && setSearch({ [dimension]: row.value })}
            className={`${BAR_ROW} cursor-pointer hover:bg-surface-hover`}
          >
            {content}
          </button>
        );
      })}
    </div>
  );
}
