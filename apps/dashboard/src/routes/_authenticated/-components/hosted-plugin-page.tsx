import type { HostComponents } from '@rustrak/plugin/ui';
import { Navigate, useNavigate } from '@tanstack/react-router';
import { type ComponentProps, useCallback } from 'react';
import {
  pageAt,
  pluginSearch,
  viewPage,
} from '@/features/plugin/model/plugins';
import { PluginPage } from '@/features/plugin/ui/components/plugin-page';
import { SavedViews } from '@/features/saved-view/ui/components/saved-views';
import { useEntryView } from '@/features/saved-view/ui/hooks/use-entry-view';

type ViewsProps = ComponentProps<HostComponents['Views']>;

/**
 * A plugin's page as both of its routes draw it, in a project or outside
 * any: its filters in the address, its saved views, opened on the default
 * view the reader chose.
 */
export function HostedPluginPage({
  pluginId,
  splat,
  projectId,
  search,
}: {
  pluginId: string;
  splat: string | undefined;
  projectId?: number;
  search: Record<string, string>;
}) {
  const navigate = useNavigate();
  // Like every other page's filters: each change is a step back can undo.
  const setSearch = useCallback(
    (next: Record<string, string>) => void navigate({ to: '.', search: next }),
    [navigate],
  );
  const at = pageAt(splat);
  const page = viewPage(pluginId, at);
  const Views = useCallback(
    ({ search, onApply }: ViewsProps) => (
      <SavedViews
        projectId={projectId}
        page={page}
        search={search}
        onApply={onApply}
      />
    ),
    [page, projectId],
  );
  const entry = useEntryView(projectId, page, search, pluginSearch);
  if (entry) return <Navigate to="." search={entry} replace />;
  return (
    <PluginPage
      pluginId={pluginId}
      at={at}
      projectId={projectId}
      search={search}
      setSearch={setSearch}
      Views={Views}
    />
  );
}
