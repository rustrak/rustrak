import {
  definePage,
  usePluginSettings,
  usePluginTranslations,
} from '@rustrak/plugin/ui';
import { Panel } from '@rustrak/ui';

export const config = definePage({
  sidebar: { label: 'nav.hello', icon: 'star' },
});

/** A page outside any project, with the greeting its settings set. */
export default function Everywhere() {
  const t = usePluginTranslations();
  const settings = usePluginSettings();
  return (
    <div className="p-page-gutter">
      <Panel title={t('instance.title')}>
        <p className="text-value text-fg-secondary" data-testid="instance-page">
          {settings.data
            ? t('instance.greeting', { greeting: settings.data.greeting })
            : '…'}
        </p>
      </Panel>
    </div>
  );
}
