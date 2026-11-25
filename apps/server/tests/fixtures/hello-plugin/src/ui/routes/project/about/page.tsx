import {
  definePage,
  type PageProps,
  usePlugin,
  usePluginTranslations,
} from '@rustrak/plugin/ui';
import { Panel } from '@rustrak/ui';

export const config = definePage({
  sidebar: { label: 'nav.about', icon: 'info', rank: 2 },
});

export default function About({ projectId }: PageProps<'project'>) {
  const t = usePluginTranslations();
  const { pluginId } = usePlugin();
  return (
    <div className="p-page-gutter">
      <Panel title={t('about.title')}>
        <p className="text-value text-fg-secondary" data-testid="about">
          {t('about.body', { plugin: pluginId, project: projectId })}
        </p>
      </Panel>
    </div>
  );
}
