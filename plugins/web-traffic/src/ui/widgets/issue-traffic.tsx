import {
  defineWidget,
  useFormatter,
  usePluginApi,
  usePluginPermissions,
  usePluginTranslations,
  useRustrakApi,
  type WidgetProps,
} from '@rustrak/plugin/ui';
import { Figure, FigureStrip, Panel } from '@rustrak/ui';
import { pathOf } from '../model';

export const config = defineWidget({ zone: 'issue.details.after' });

interface Page<T> {
  items: T[];
}

/**
 * How many people saw the page an issue happens on: what the error weighs.
 * Drawn only when the issue's latest event names a page.
 */
export default function IssueTraffic({
  projectId,
  issue,
}: WidgetProps<'issue.details.after'>) {
  const base = `/api/projects/${projectId}/issues/${issue.id}/events`;
  const latest = useRustrakApi<Page<{ id: string }>>(base);
  const id = latest.data?.items[0]?.id;
  const event = useRustrakApi<{ data: { request?: { url?: string } } }>(
    `${base}/${id}`,
    {
      enabled: Boolean(id),
    },
  );
  const path = pathOf(event.data?.data.request?.url);
  // Only for whoever may read the traffic: the route would refuse the rest.
  const can = usePluginPermissions().can('traffic', 'read');
  return path && can ? <PageTraffic path={path} /> : null;
}

function PageTraffic({ path }: { path: string }) {
  const t = usePluginTranslations();
  const format = useFormatter();
  const views = usePluginApi<{ pageviews: number; visitors: number }>('/page', {
    path,
    period: '7d',
  });
  return (
    <Panel title={t('widget.issue', { path })} hint={t('widget.issueHint')}>
      <FigureStrip className="shrink-0 grid-cols-2">
        <Figure
          label={t('overview.visitors')}
          value={views.data ? format.number(views.data.visitors) : '…'}
        />
        <Figure
          label={t('overview.pageviews')}
          value={views.data ? format.number(views.data.pageviews) : '…'}
        />
      </FigureStrip>
    </Panel>
  );
}
