/**
 * Plugins' dashboard code: each bundle is a script that registers itself.
 *
 * A bundle never carries React: it reads the dashboard's own from
 * `globalThis.__rustrak`, so a plugin's hooks run in the same React and its
 * reads share the dashboard's query cache. The bundle's URL carries its
 * version, so a loaded one is good until the plugin is updated.
 */
import type { PluginSummary } from '@rustrak/client';
import type { PluginDefinition } from '@rustrak/plugin/ui';
import * as ui from '@rustrak/plugin/ui';
import * as designSystem from '@rustrak/ui';
import * as ReactQuery from '@tanstack/react-query';
import { noop, useQueries, useQuery } from '@tanstack/react-query';
import * as React from 'react';
import { useEffect } from 'react';
import * as jsxRuntime from 'react/jsx-runtime';
import * as ReactDOM from 'react-dom';
import type { ApiError } from '@/shared/api/query';
import { pluginQueries } from '../../api/queries';
import { newRules, ruleKeys } from '../../model/styles';

declare global {
  var __rustrak: unknown;
  var __rustrakPlugins:
    | { register: (id: string, definition: PluginDefinition) => void }
    | undefined;
}

const waiting = new Map<string, (definition: PluginDefinition) => void>();

function expose() {
  if (globalThis.__rustrakPlugins) return;
  globalThis.__rustrak = {
    React,
    ReactDOM,
    jsxRuntime,
    ReactQuery,
    ui,
    designSystem,
  };
  globalThis.__rustrakPlugins = {
    register: (id, definition) => {
      waiting.get(id)?.(definition);
      waiting.delete(id);
    },
  };
}

/** The dashboard's rules, from every stylesheet it can read. */
function dashboardRules(): Set<string> {
  const keys = new Set<string>();
  for (const sheet of Array.from(document.styleSheets)) {
    if ((sheet.ownerNode as Element | null)?.hasAttribute('data-plugin'))
      continue;
    try {
      for (const key of ruleKeys(sheet.cssRules)) keys.add(key);
    } catch {
      // Another origin's sheet: nothing a plugin could copy.
    }
  }
  return keys;
}

/**
 * The plugin's stylesheet: only its own classes, on the dashboard's tokens,
 * without the copies that would reorder the dashboard's (`model/styles.ts`).
 */
async function loadStyles(id: string, href: string | null | undefined) {
  const current = document.querySelector<HTMLStyleElement>(
    `style[data-plugin="${id}"]`,
  );
  if (!href || current?.dataset.href === href) return;
  const response = await fetch(href);
  if (!response.ok) throw new Error(`Could not load ${href}`);
  const sheet = new CSSStyleSheet();
  sheet.replaceSync(await response.text());
  const style = document.createElement('style');
  style.dataset.plugin = id;
  // An update's bundle has a new address: its sheet replaces the old one.
  style.dataset.href = href;
  style.textContent = newRules(sheet.cssRules, dashboardRules());
  current?.remove();
  document.head.append(style);
}

/** Each plugin's latest load: one registers at a time, as `waiting` holds one. */
const loads = new Map<string, Promise<unknown>>();

function loadBundle(
  id: string,
  entry: string,
  styles?: string | null,
): Promise<PluginDefinition> {
  const next = (loads.get(id) ?? Promise.resolve())
    .catch(noop)
    .then(() => loadOnce(id, entry, styles));
  loads.set(id, next);
  return next;
}

function loadOnce(
  id: string,
  entry: string,
  styles?: string | null,
): Promise<PluginDefinition> {
  expose();
  const css = loadStyles(id, styles);
  const code = new Promise<PluginDefinition>((resolve, reject) => {
    waiting.set(id, resolve);
    const script = document.createElement('script');
    script.src = entry;
    script.async = true;
    script.onload = () => {
      if (waiting.has(id)) {
        waiting.delete(id);
        reject(new Error(`${entry} did not register plugin ${id}`));
      }
    };
    script.onerror = () => {
      waiting.delete(id);
      reject(new Error(`Could not load ${entry}`));
    };
    document.head.append(script);
  });
  return Promise.all([code, css]).then(([definition]) => definition);
}

export interface LoadedPlugin {
  plugin: PluginSummary;
  definition: PluginDefinition;
}

/** The plugins with dashboard code, loaded; `pending` while any is not yet. */
export function useLoadedPlugins(): {
  loaded: LoadedPlugin[];
  /** The plugins whose bundle would not load or register. */
  failed: string[];
  pending: boolean;
  /** Why the list of plugins could not be read. */
  error: ApiError | null;
} {
  const list = useQuery(pluginQueries.list());
  const withUi = (list.data ?? []).flatMap((p) =>
    p.ui ? [{ ...p, ui: p.ui }] : [],
  );
  const bundles = useQueries({
    queries: withUi.map((p) => ({
      // Its own root: refreshing the list must not run a bundle twice.
      queryKey: ['plugin-bundles', p.ui.entry],
      queryFn: () =>
        loadBundle(p.id, p.ui.entry, p.ui.styles).catch((error: unknown) => {
          console.error(`Plugin ${p.id} failed to load`, error);
          throw error;
        }),
      staleTime: Number.POSITIVE_INFINITY,
      gcTime: Number.POSITIVE_INFINITY,
    })),
  });
  const loaded = withUi.flatMap((plugin, i) => {
    const definition = bundles[i]?.data;
    return definition ? [{ plugin, definition }] : [];
  });
  // A bundle read from the cache (a plugin installed again, an earlier
  // version back) skips `loadStyles`; its sheet may be another version's.
  const sheets = loaded
    .map(({ plugin }) => `${plugin.id} ${plugin.ui.styles ?? ''}`)
    .join('\n');
  useEffect(() => {
    for (const line of sheets.split('\n')) {
      const [id, href] = line.split(' ');
      if (id) void loadStyles(id, href).catch(noop);
    }
  }, [sheets]);
  return {
    loaded,
    error: list.error,
    failed: withUi.filter((_, i) => bundles[i]?.isError).map((p) => p.id),
    pending: list.isPending || bundles.some((b) => b.isPending),
  };
}
