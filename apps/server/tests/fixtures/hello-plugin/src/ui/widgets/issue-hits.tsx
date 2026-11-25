import {
  defineWidget,
  usePluginApi,
  usePluginTranslations,
  type WidgetProps,
} from '@rustrak/plugin/ui';
import { Figure, FigureStrip, Panel } from '@rustrak/ui';

export const config = defineWidget({ zone: 'issue.details.after' });

/** Drawn under every issue of a project where Hello is active. */
export default function IssueHits({
  issue,
}: WidgetProps<'issue.details.after'>) {
  const t = usePluginTranslations();
  const count = usePluginApi<string>('/count');
  return (
    <Panel title={t('widget.title', { issue: issue.title })}>
      <FigureStrip className="grid-cols-1">
        <Figure label={t('widget.hits')} value={count.data ?? '…'} />
      </FigureStrip>
    </Panel>
  );
}
