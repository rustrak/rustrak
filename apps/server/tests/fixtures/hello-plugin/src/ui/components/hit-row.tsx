import {
  PluginLink,
  useFormatter,
  usePluginTranslations,
} from '@rustrak/plugin/ui';
import { SettingsListItem } from '@rustrak/ui';

export interface Hit {
  id: number;
  path: string;
  createdAt: number;
}

/** One hit in a list: its path, when, and the way to its own page. */
export function HitRow({ hit }: { hit: Hit }) {
  const t = usePluginTranslations();
  const format = useFormatter();
  return (
    <SettingsListItem
      title={hit.path}
      description={format.dateTime(new Date(hit.createdAt * 1000), 'dateTime')}
      actions={
        <PluginLink to={`hits/${hit.id}`} className="text-meta text-fg-brand">
          {t('hits.open')}
        </PluginLink>
      }
    />
  );
}
