import type { PluginSummary } from '@rustrak/client';
import {
  type HostComponents,
  type PeriodFilterProps,
  pluginTranslator,
} from '@rustrak/plugin/ui';
import { EmptyState, Skeleton } from '@rustrak/ui';
import { useCallback, useMemo, useState } from 'react';
import { useLocale, useTranslations } from 'use-intl';
import type { PeriodChoice } from '@/shared/lib/period';
import { LoadFailure } from '@/shared/ui/components/load-failure';
import { NotAuthorized } from '@/shared/ui/components/not-authorized';
import { PeriodFilter } from '@/shared/ui/components/period-filter';
import {
  type TrailCrumb,
  usePageTrail,
} from '@/shared/ui/hooks/use-page-trail';
import { usePermissions } from '@/shared/ui/hooks/use-permissions';
import {
  activeIn,
  bySidebarRank,
  matchPage,
  pageAllowed,
  pageHref,
  sectionOf,
} from '../../model/plugins';
import {
  type LoadedPlugin,
  useLoadedPlugins,
} from '../hooks/use-loaded-plugins';
import { PluginBoundary, PluginFailed } from './plugin-boundary';
import { PluginScope } from './plugin-scope';

/** One page of a plugin: in a project, or outside any. */
export function PluginPage({
  pluginId,
  at,
  projectId,
  search,
  setSearch,
  Views,
}: {
  pluginId: string;
  /** The page's address under the plugin, `funnels/42`. */
  at: string;
  /** In this project; outside any when absent. */
  projectId?: number;
  /** The address query: the page's filters. */
  search: Record<string, string>;
  setSearch: (next: Record<string, string>) => void;
  /** The page's saved views menu: the route's, which knows the project. */
  Views: HostComponents['Views'];
}) {
  const t = useTranslations('settings');
  const scope = projectId === undefined ? 'instance' : 'project';
  const components = useMemo(
    () => ({ Views, PeriodFilter: HostPeriod }),
    [Views],
  );
  const { loaded, failed, pending, error } = useLoadedPlugins();
  const entry = loaded.find(({ plugin }) => plugin.id === pluginId);
  const found = entry ? matchPage(entry.definition.pages, scope, at) : null;
  const usable =
    found && (projectId === undefined || activeIn(entry!.plugin, projectId));
  const { title, setTrail } = usePluginPageTrail({
    entry,
    path: found?.page.path,
    scope,
    at,
    pluginId,
    projectId,
    shown: Boolean(usable),
  });
  const allowed = usePageAllowed(
    entry?.plugin,
    found?.page.path,
    scope,
    projectId,
  );

  if (!usable) {
    if (failed.includes(pluginId)) return <PluginFailed pluginId={pluginId} />;
    if (pending) return <PluginPageSkeleton />;
    if (error) {
      return (
        <LoadFailure
          error={error.error}
          title={t('plugins.listFailed')}
          notFoundOnMissing={false}
        />
      );
    }
    return (
      <EmptyState
        title={t('plugins.pageMissing')}
        description={t('plugins.pageMissingHint')}
      />
    );
  }
  if (!allowed) {
    return (
      <NotAuthorized scope={scope === 'project' ? 'project' : 'workspace'} />
    );
  }
  const Page = found.page.component;
  return (
    <PluginScope
      pluginId={pluginId}
      projectId={projectId}
      messages={entry!.definition.messages}
      search={search}
      setSearch={setSearch}
      setTrail={setTrail}
      components={components}
    >
      <title>{`${title} | Rustrak`}</title>
      <PluginBoundary
        key={`${pluginId}:${projectId ?? ''}:${at}`}
        pluginId={pluginId}
      >
        <Page projectId={projectId} params={found.params} />
      </PluginBoundary>
    </PluginScope>
  );
}

/** A plugin page while its bundle or the reader's permissions load. */
export function PluginPageSkeleton() {
  return (
    <div className="p-6">
      <Skeleton className="h-40 w-full rounded-lg" />
    </div>
  );
}

/** The dashboard's window control, lent to plugin pages. */
function HostPeriod({ value, onChange, choices }: PeriodFilterProps) {
  return (
    <PeriodFilter
      period={value as PeriodChoice}
      onChange={onChange}
      choices={choices as readonly PeriodChoice[] | undefined}
    />
  );
}

/**
 * A plugin page's breadcrumbs: the plugin, leading to its first page; the
 * sidebar page it sits under, leading back to it from one of its details;
 * then what the page names itself (`usePluginTrail`). Returns the page's
 * title and the setter it is lent.
 */
function usePluginPageTrail({
  entry,
  path,
  scope,
  at,
  pluginId,
  projectId,
  shown,
}: {
  entry: LoadedPlugin | undefined;
  path: string | undefined;
  scope: 'project' | 'instance';
  at: string;
  pluginId: string;
  projectId?: number;
  shown: boolean;
}) {
  const locale = useLocale();
  const pages = (entry?.plugin.ui?.pages ?? []).filter(
    (p) => p.scope === scope,
  );
  const section = sectionOf(pages, path ?? '');
  const first = pages.filter((p) => p.sidebar).sort(bySidebarRank)[0];
  const name = entry?.plugin.name ?? pluginId;
  const [own, setOwn] = useState<{ at: string; crumbs: TrailCrumb[] }>();
  const setTrail = useCallback(
    (crumbs: TrailCrumb[]) => setOwn({ at, crumbs }),
    [at],
  );
  const crumbs = own?.at === at ? own.crumbs : [];
  const after: TrailCrumb[] = [];
  if (entry && section?.sidebar) {
    after.push({
      label: pluginTranslator(
        entry.definition.messages,
        locale,
      )(section.sidebar.label),
      ...(section.path !== path || crumbs.length > 0
        ? { href: pageHref(pluginId, section.path, projectId) }
        : {}),
    });
  }
  after.push(...crumbs);
  // The page you are on is never a link: the plugin's name, only on its way.
  const trail: TrailCrumb[] = [
    after.length > 0
      ? { label: name, href: pageHref(pluginId, first?.path ?? '', projectId) }
      : { label: name },
    ...after,
  ];
  usePageTrail(shown ? trail : []);
  return { title: trail.at(-1)!.label, setTrail };
}

/** Whether the reader holds what the plugin's page requires, if anything. */
function usePageAllowed(
  plugin: PluginSummary | undefined,
  path: string | undefined,
  scope: 'project' | 'instance',
  projectId?: number,
): boolean {
  const { canPlugin } = usePermissions();
  const page = plugin?.ui?.pages.find(
    (p) => p.scope === scope && p.path === path,
  );
  return !plugin || !page || pageAllowed(page, plugin, canPlugin, projectId);
}
