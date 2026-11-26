import {
  definePage,
  type PageProps,
  usePluginTranslations,
} from '@rustrak/plugin/ui';
import { Panel } from '@rustrak/ui';
import { Greeting } from '../../components/greeting';

export const config = definePage({
  sidebar: { label: 'nav.visits', icon: 'link', rank: 1 },
});

export default function Visits({ projectId }: PageProps<'project'>) {
  const t = usePluginTranslations();
  return (
    <Panel title={t('nav.visits')}>
      <Greeting />
      <span className="text-fg-muted">{`project ${projectId}`}</span>
    </Panel>
  );
}
