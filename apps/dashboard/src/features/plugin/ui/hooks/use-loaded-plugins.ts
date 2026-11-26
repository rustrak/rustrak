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

/** Each fetched stylesheet's text, by its address (which carries its version). */
const fetchedStyles = new Map<string, string>();

async function fetchStyles(href: string, signal: AbortSignal): Promise<true> {
  const response = await fetch(href, { signal });
  if (!response.ok) throw new Error(`Could not load ${href}`);
  fetchedStyles.set(href, await response.text());
  return true;
}

/**
 * Makes the page carry exactly these plugins' sheets (`id href` per line):
 * each one only its own classes, on the dashboard's tokens, without the
 * copies that would reorder the dashboard's (`model/styles.ts`). Another
 * version's sheet, or one of a plugin no longer loaded, goes.
 */
function applyStyles(sheets: string) {
  const wanted = new Map(
    sheets
      .split('\n')
      .filter(Boolean)
      .map((line) => line.split(' ') as [string, string]),
  );
  for (const style of document.querySelectorAll<HTMLStyleElement>(
    'style[data-plugin]',
  )) {
    const id = style.dataset.plugin ?? '';
    if (wanted.get(id) === style.dataset.href) wanted.delete(id);
    else style.remove();
  }
  for (const [id, href] of wanted) {
    const sheet = new CSSStyleSheet();
    sheet.replaceSync(fetchedStyles.get(href) ?? '');
    const style = document.createElement('style');
    style.dataset.plugin = id;
    style.dataset.href = href;
    style.textContent = newRules(sheet.cssRules, dashboardRules());
    document.head.append(style);
  }
}

/**
 * Each plugin's latest load: one registers at a time, as `waiting` holds one.
 * A load settles only once its script has, so the next never takes its turn
 * early.
 */
const loads = new Map<string, Promise<unknown>>();

function loadBundle(id: string, entry: string): Promise<PluginDefinition> {
  const next = (loads.get(id) ?? Promise.resolve())
    .catch(noop)
    .then(() => loadOnce(id, entry));
  loads.set(id, next);
  return next;
}

function loadOnce(id: string, entry: string): Promise<PluginDefinition> {
  expose();
  return new Promise<PluginDefinition>((resolve, reject) => {
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
        loadBundle(p.id, p.ui.entry).catch((error: unknown) => {
          console.error(`Plugin ${p.id} failed to load`, error);
          throw error;
        }),
      staleTime: Number.POSITIVE_INFINITY,
      gcTime: Number.POSITIVE_INFINITY,
    })),
  });
  // A stylesheet is read like any other resource; putting it on the page
  // is the effect below, which only ever reads what has arrived.
  const styles = useQueries({
    queries: withUi.map((p) => ({
      queryKey: ['plugin-styles', p.ui.styles],
      queryFn: ({ signal }: { signal: AbortSignal }) =>
        fetchStyles(p.ui.styles ?? '', signal),
      enabled: Boolean(p.ui.styles),
      staleTime: Number.POSITIVE_INFINITY,
      gcTime: Number.POSITIVE_INFINITY,
    })),
  });
  const loaded = withUi.flatMap((plugin, i) => {
    const definition = bundles[i]?.data;
    return definition ? [{ plugin, definition }] : [];
  });
  // The sheets the page should carry: each loaded plugin's own version.
  const sheets = withUi
    .filter((p, i) => bundles[i]?.data && styles[i]?.data && p.ui.styles)
    .map((p) => `${p.id} ${p.ui.styles}`)
    .join('\n');
  useEffect(() => applyStyles(sheets), [sheets]);
  return {
    loaded,
    error: list.error,
    failed: withUi.filter((_, i) => bundles[i]?.isError).map((p) => p.id),
    pending:
      list.isPending ||
      bundles.some((b) => b.isPending) ||
      withUi.some((p, i) => p.ui.styles && styles[i]?.isPending),
  };
}
