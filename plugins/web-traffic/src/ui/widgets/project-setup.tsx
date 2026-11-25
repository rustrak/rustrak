import {
  defineWidget,
  publicUrl,
  usePlugin,
  usePluginTranslations,
  useRustrakApi,
  type WidgetProps,
} from '@rustrak/plugin/ui';
import { keyOf } from '../model';

export const config = defineWidget({ zone: 'plugin.setup' });

/** A block of code, as the setup shows it. */
function Code({ children }: { children: string }) {
  return (
    <pre className="overflow-x-auto rounded-lg border border-border-subtle bg-surface px-4 py-3 font-mono text-meta text-fg-secondary">
      {children}
    </pre>
  );
}

/**
 * How a project's website is counted: the snippet with this project's key,
 * its own events, and what is kept. In the plugin's settings, per project.
 */
export default function ProjectSetup({
  projectId,
}: WidgetProps<'plugin.setup'>) {
  const t = usePluginTranslations();
  const plugin = usePlugin();
  const project = useRustrakApi<{ dsn: string | null }>(
    `/api/projects/${projectId}`,
  );
  const key = keyOf(project.data?.dsn);
  const snippet = `<script defer src="${publicUrl(plugin, 'script.js')}"
  data-project="${projectId}" data-key="${key ?? '…'}"></script>`;

  return (
    <div className="flex flex-col gap-4">
      <section className="flex flex-col gap-2">
        <h3 className="text-value font-medium text-fg">{t('setup.snippet')}</h3>
        <p className="text-meta text-fg-subtle">{t('setup.snippetHint')}</p>
        {project.data && !key ? (
          <p role="alert" className="text-meta text-danger-fg">
            {t('setup.noKey')}
          </p>
        ) : (
          <Code>{snippet}</Code>
        )}
      </section>
      <section className="flex flex-col gap-2">
        <h3 className="text-value font-medium text-fg">{t('setup.events')}</h3>
        <p className="text-meta text-fg-subtle">{t('setup.eventsHint')}</p>
        <Code>{`webTraffic.track('signup', { plan: 'pro' });
<button data-traffic-event="buy">Buy</button>`}</Code>
      </section>
      <p className="text-meta text-fg-subtle">{t('setup.privacyHint')}</p>
    </div>
  );
}
