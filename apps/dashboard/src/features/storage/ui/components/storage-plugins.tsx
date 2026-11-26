import type { PluginStorage } from '@rustrak/client';
import {
  SettingSection,
  SettingsEmpty,
  SettingsListItem,
  SettingsListSkeleton,
} from '@rustrak/ui';
import { useQuery } from '@tanstack/react-query';
import { useFormatter, useTranslations } from 'use-intl';
import { storageQueries } from '@/features/storage/api/queries';
import { formatBytes } from '@/shared/lib/utils';
import { QueryState } from '@/shared/ui/components/query-state';

/**
 * Each plugin's data: its file against its quota, and its history (its
 * tables with `created_at`). How long that is kept is set in the retention
 * dialog, and deleting it now in the delete dialog, with Rustrak's own.
 */
export function StoragePlugins() {
  const t = useTranslations('storage');
  const plugins = useQuery(storageQueries.plugins());
  return (
    <SettingSection
      title={t('plugins.title')}
      description={t('plugins.description')}
    >
      <QueryState
        query={plugins}
        placeholder={<SettingsListSkeleton rows={1} />}
        title={t('plugins.loadFailed')}
      >
        {(list) =>
          list.length === 0 ? (
            <SettingsEmpty title={t('plugins.none')} />
          ) : (
            list.map((plugin) => <PluginRow key={plugin.id} plugin={plugin} />)
          )
        }
      </QueryState>
    </SettingSection>
  );
}

function PluginRow({ plugin }: { plugin: PluginStorage }) {
  const t = useTranslations('storage');
  const format = useFormatter();
  const rows = plugin.history.reduce((n, h) => n + h.rows, 0);
  const facts = [
    t('plugins.size', {
      used: formatBytes(plugin.bytes),
      quota: formatBytes(plugin.quota_bytes),
    }),
    plugin.history.length === 0
      ? t('plugins.noHistory')
      : t('plugins.history', {
          rows: format.number(rows),
          tables: plugin.history.map((h) => h.table).join(', '),
        }),
  ];
  return (
    <SettingsListItem
      title={plugin.name}
      description={facts.join(' · ')}
      meta={
        plugin.history.length === 0
          ? undefined
          : plugin.retention_days === null
            ? t('keep.forever')
            : t('keep.days', { count: plugin.retention_days })
      }
    />
  );
}

/** In a dialog: the plugins' read failed, so their history is not offered. */
export function PluginsLoadFailed() {
  const t = useTranslations('storage');
  return (
    <p role="alert" className="text-meta text-danger-fg">
      {t('plugins.loadFailed')}
    </p>
  );
}
