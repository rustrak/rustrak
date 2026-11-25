import { Component, type ReactNode } from 'react';
import { useTranslations } from 'use-intl';

/** What stands for a plugin's page or widget that failed. */
export function PluginFailed({ pluginId }: { pluginId: string }) {
  const t = useTranslations('settings');
  return (
    <p className="p-6 text-meta text-danger-fg">
      {t('plugins.failed', { plugin: pluginId })}
    </p>
  );
}

/**
 * Keeps a plugin's failure inside its own box: the dashboard page around it
 * keeps working.
 */
export class PluginBoundary extends Component<
  { pluginId: string; children: ReactNode },
  { failed: boolean }
> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  componentDidCatch(error: unknown) {
    console.error('A plugin failed to render', error);
  }

  render() {
    return this.state.failed ? (
      <PluginFailed pluginId={this.props.pluginId} />
    ) : (
      this.props.children
    );
  }
}
