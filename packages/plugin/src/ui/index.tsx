// What a plugin's React code imports: `@rustrak/plugin/ui`.
//
// A plugin never bundles this: the dashboard loads it once and hands it to
// every plugin, together with its own React, query cache and translations, so
// a plugin page uses the very components the dashboard is built from and is
// indistinguishable from the rest of Rustrak.

import type { icons } from '@rustrak/ui';
import {
  type UseMutationResult,
  type UseQueryResult,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query';
import {
  type ComponentType,
  createContext,
  type ReactNode,
  useContext,
  useEffect,
  useMemo,
} from 'react';
import { createTranslator, useLocale } from 'use-intl';
import type { PermissionAction, Scope } from '../config.ts';
import type { Messages } from '../messages.ts';

/**
 * Dates and numbers in the reader's language, zone and clock, through the
 * dashboard's named formats: `format.dateTime(date, 'dateTime')`, `'date'`,
 * `'precise'`; `format.number(n, 'compact')`.
 */
export { useFormatter, useLocale } from 'use-intl';

export type { Messages, PermissionAction, Scope };

/** Where a plugin's component is drawn. */
export interface PluginContextValue {
  pluginId: string;
  /** The project, on project pages and widgets. */
  projectId?: number;
  /** The plugin's messages, by language. */
  messages: Record<string, Messages>;
  /** The dashboard's router: goes to an address without reloading. */
  navigate: (href: string) => void;
  /** The reader's permissions, as the dashboard knows them. */
  can: (key: string, action: PermissionAction, projectId?: number) => boolean;
  /** The page's address query: its filters. Absent in a widget. */
  search?: Record<string, string>;
  /** Replaces the page's address query. */
  setSearch?: (next: Record<string, string>) => void;
  /** Names the page in the breadcrumbs, after its section. */
  setTrail?: (crumbs: TrailCrumb[]) => void;
  /** The dashboard's own controls, so a plugin's page looks like the rest. */
  components?: HostComponents;
}

/** A crumb as the dashboard draws it. */
export interface TrailCrumb {
  label: string;
  href?: string;
  /** An identifier the system chose, set in mono. */
  mono?: boolean;
}

/** A crumb a plugin page names: `to` is one of the plugin's own pages. */
export interface PluginCrumb {
  label: string;
  to?: string;
  search?: Search;
  mono?: boolean;
}

/** Controls the dashboard lends a plugin's pages. */
export interface HostComponents {
  /** The saved views menu of the page, over its address query. */
  Views: ComponentType<{
    search: Record<string, string>;
    onApply: (search: Record<string, string>) => void;
  }>;
  /** The time window, as every project page shows it. */
  PeriodFilter: ComponentType<PeriodFilterProps>;
}

export interface PeriodFilterProps<Period extends string = string> {
  value: Period;
  onChange: (value: Period) => void;
  /** The windows on offer, like `24h`, `7d`, `30d`; the dashboard's by default. */
  choices?: readonly Period[];
}

/**
 * The plugin's permissions, typed from rustrak.config.ts: each resource and
 * the actions it declares. `rustrak-plugin generate` fills this in.
 */
// biome-ignore lint/suspicious/noEmptyInterface: augmented per plugin
export interface PluginPermissions {}

type PermissionResource = [keyof PluginPermissions] extends [never]
  ? string
  : Extract<keyof PluginPermissions, string>;

export const PluginContext = createContext<PluginContextValue | null>(null);

/** The plugin and project this component is drawn for. */
export function usePlugin(): PluginContextValue {
  const ctx = useContext(PluginContext);
  if (!ctx) {
    throw new Error('usePlugin is only available inside a Rustrak plugin');
  }
  return ctx;
}

/**
 * The plugin's message keys. `rustrak-plugin generate` fills this in from
 * `src/messages/en.json`, so `t('nav.funnels')` and a sidebar `label` are
 * checked by the compiler.
 */
// biome-ignore lint/suspicious/noEmptyInterface: augmented per plugin
export interface PluginMessages {}

export type MessageKey = [keyof PluginMessages] extends [never]
  ? string
  : Extract<keyof PluginMessages, string>;

/**
 * The plugin's settings as the browser sees them (secrets are never sent).
 * `rustrak-plugin generate` fills this in from `settings` in rustrak.config.ts.
 */
// biome-ignore lint/suspicious/noEmptyInterface: augmented per plugin
export interface PluginSettings {}

type IconName = keyof typeof icons;

/** What each scope's pages receive. `params` are the `[name]` folders. */
export interface ScopeProps {
  project: { projectId: number; params: Record<string, string> };
  instance: { params: Record<string, string> };
}

export type PageProps<S extends Scope> = ScopeProps[S];

export interface PageConfig {
  /** Show the page in the sidebar of its scope, ordered by `rank`. */
  sidebar?: { label: MessageKey; icon?: IconName; rank?: number };
  /**
   * One of the plugin's permissions, `visitors:read`: the page, and its
   * place in the sidebar, are only for whoever holds it.
   */
  requires?: `${PermissionResource}:${PermissionAction}`;
}

/** A page's configuration: `export const config = definePage({ … })`. */
export function definePage(config: PageConfig): PageConfig {
  return config;
}

/** The places in the dashboard's own pages a plugin can draw into. */
export interface ZoneProps {
  /** Under an issue's charts, on every issue of a project. */
  'issue.details.after': {
    projectId: number;
    issue: { id: string; title: string };
  };
  /** At the end of a project's overview. */
  'project.overview.after': { projectId: number };
  /**
   * In the plugin's own settings, under each project it runs in: how to set
   * it up there (a snippet with the project's key, say).
   */
  'plugin.setup': { projectId: number };
}

export type Zone = keyof ZoneProps;
export type WidgetProps<Z extends Zone> = ZoneProps[Z];

export interface WidgetConfig<Z extends Zone = Zone> {
  zone: Z;
}

/** A widget's configuration: `export const config = defineWidget({ zone })`. */
export function defineWidget<Z extends Zone>(
  config: WidgetConfig<Z>,
): WidgetConfig<Z> {
  return config;
}

export interface PluginPage {
  /** Under the plugin's URL; `:name` for a `[name]` folder. */
  path: string;
  scope: Scope;
  config?: PageConfig;
  component: ComponentType<any>;
}

export interface PluginWidget {
  zone: Zone;
  component: ComponentType<any>;
}

/** What a plugin's bundle registers; written by `rustrak-plugin build`. */
export interface PluginDefinition {
  pages: PluginPage[];
  widgets: PluginWidget[];
  messages: Record<string, Messages>;
}

function merge(base: Messages, over: Messages): Messages {
  const out: Messages = { ...base };
  for (const [key, value] of Object.entries(over)) {
    const under = out[key];
    out[key] =
      typeof value === 'object' && typeof under === 'object'
        ? merge(under, value)
        : value;
  }
  return out;
}

/** A translator over a plugin's messages: `locale`, English where it has none. */
export function pluginTranslator(
  messages: Record<string, Messages> | undefined,
  locale: string,
): (
  key: MessageKey,
  values?: Record<string, string | number | Date>,
) => string {
  const merged = merge(messages?.en ?? {}, messages?.[locale] ?? {});
  const t = createTranslator({ locale, messages: merged });
  return (key, values) => t(key as never, values as never);
}

/**
 * The plugin's own messages in the reader's language, English where that
 * language has no entry. Same ICU syntax as the dashboard: `{name}`, plurals.
 */
export function usePluginTranslations(): ReturnType<typeof pluginTranslator> {
  const { messages } = usePlugin();
  const locale = useLocale();
  return useMemo(() => pluginTranslator(messages, locale), [messages, locale]);
}

type Search = Record<string, string | number | boolean | undefined>;

/** `?a=1&b=x`, leaving out what is unset or empty; `''` with nothing. */
function queryString(search: Search = {}): string {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(search)) {
    if (value !== undefined && value !== '') params.set(key, String(value));
  }
  return params.size > 0 ? `?${params}` : '';
}

type Where = Pick<PluginContextValue, 'pluginId' | 'projectId'>;

/** Under a project when there is one: `/projects/3<rest>`, else `<rest>`. */
const scoped = (ctx: Where, rest: string) =>
  ctx.projectId === undefined ? rest : `/projects/${ctx.projectId}${rest}`;

/** The address of one of the plugin's own pages: `hits/7`, `''` its first. */
export function pluginHref(ctx: Where, page: string, search?: Search): string {
  const base = scoped(ctx, `/plugins/${ctx.pluginId}`);
  return `${page ? `${base}/${page}` : base}${queryString(search)}`;
}

/** The URL of one of the plugin's own routes. */
const pluginUrl = (ctx: Where, path: string, search?: Search) =>
  `/api${scoped(ctx, `/plugins/${ctx.pluginId}`)}${path}${queryString(search)}`;

/** Where the browser reads the plugin's settings, here. */
const settingsUrl = (ctx: Where) =>
  ctx.projectId === undefined
    ? `/api/settings/plugins/${ctx.pluginId}/settings`
    : `/api/projects/${ctx.projectId}/plugin-settings/${ctx.pluginId}`;

/**
 * The query cache's keys for a plugin, shared with the dashboard: what its
 * routes answer, and its settings as the settings form edits them.
 */
export const pluginKeys = {
  all: (pluginId: string) => ['plugin', pluginId] as const,
  route: (ctx: Where, path: string, search?: Search) =>
    ['plugin', ctx.pluginId, ctx.projectId, path, search] as const,
  settings: (pluginId: string, projectId?: number) =>
    ['plugin', pluginId, 'settings', projectId ?? 'instance'] as const,
};

/**
 * A link to anywhere in the dashboard, through its router: a plain click
 * stays in the page, a click that asks for a new tab gets one.
 */
export function HostLink({
  href,
  className,
  children,
}: {
  href: string;
  className?: string;
  children: ReactNode;
}) {
  const { navigate } = usePlugin();
  return (
    <a
      href={href}
      className={className}
      onClick={(e) => {
        if (e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
        e.preventDefault();
        navigate(href);
      }}
    >
      {children}
    </a>
  );
}

/** A link to another of the plugin's pages, through the dashboard's router. */
export function PluginLink({
  to,
  search,
  className,
  children,
}: {
  /** The page under the plugin: `hits/7`, `''` for its first. */
  to: string;
  /** Its address query: the filters it opens with. */
  search?: Search;
  className?: string;
  children: ReactNode;
}) {
  return (
    <HostLink href={pluginHref(usePlugin(), to, search)} className={className}>
      {children}
    </HostLink>
  );
}

/** Goes to one of the plugin's pages from code, after a save. */
export function usePluginNavigate(): (page: string, search?: Search) => void {
  const ctx = usePlugin();
  return (page, search) => ctx.navigate(pluginHref(ctx, page, search));
}

/** A plugin's permission as Rustrak keys it: `<plugin>:<resource>`. */
export const pluginPermissionKey = (pluginId: string, resource: string) =>
  `${pluginId}:${resource}`;

/**
 * Whether the reader holds one of the plugin's permissions here: to hide a
 * button they could not use. The server checks again, on every route.
 */
export function usePluginPermissions(): {
  can: (resource: PermissionResource, action: PermissionAction) => boolean;
} {
  const ctx = usePlugin();
  return {
    can: (resource, action) =>
      ctx.can(
        pluginPermissionKey(ctx.pluginId, resource),
        action,
        ctx.projectId,
      ),
  };
}

/**
 * The address of one of the plugin's public files (`src/public/`), the
 * one a website loads: absolute, on this Rustrak.
 */
export function publicUrl(
  ctx: Pick<PluginContextValue, 'pluginId'>,
  file: string,
): string {
  return `${window.location.origin}/public/${ctx.pluginId}/${file}`;
}

/** A page's crumbs with their addresses resolved on this plugin. */
function trailOf(
  ctx: Pick<PluginContextValue, 'pluginId' | 'projectId'>,
  crumbs: readonly PluginCrumb[],
): TrailCrumb[] {
  return crumbs.map(({ label, to, search, mono }) => ({
    label,
    ...(to === undefined ? {} : { href: pluginHref(ctx, to, search) }),
    ...(mono ? { mono } : {}),
  }));
}

/**
 * Names the page in the breadcrumbs, after the section the sidebar puts it
 * in: an event's name, a visitor. The section becomes the way back.
 */
export function usePluginTrail(crumbs: readonly PluginCrumb[]): void {
  const ctx = usePlugin();
  const key = JSON.stringify(trailOf(ctx, crumbs));
  const set = ctx.setTrail;
  useEffect(() => {
    set?.(JSON.parse(key) as TrailCrumb[]);
  }, [key, set]);
}

/**
 * The page's filters, kept in its address so a link or a saved view brings
 * them back: `[search, setSearch]`, where `setSearch` merges and an
 * `undefined` value removes a key.
 */
export function usePluginSearch<Key extends string = string>(): [
  Partial<Record<Key, string>>,
  (next: Partial<Record<Key, string | undefined>>) => void,
] {
  const ctx = usePlugin();
  const search = (ctx.search ?? {}) as Partial<Record<Key, string>>;
  const set = (next: Partial<Record<Key, string | undefined>>) => {
    const merged: Record<string, string> = {};
    for (const [key, value] of Object.entries({ ...search, ...next })) {
      if (typeof value === 'string' && value !== '') merged[key] = value;
    }
    ctx.setSearch?.(merged);
  };
  return [search, set];
}

/** The page's saved views, the same menu as the dashboard's own pages. */
export function PluginViews() {
  const ctx = usePlugin();
  const Views = ctx.components?.Views;
  if (!Views || !ctx.search) return null;
  return (
    <Views search={ctx.search} onApply={(next) => ctx.setSearch?.(next)} />
  );
}

/** The time window control the dashboard's project pages use. */
export function PeriodFilter<Period extends string>(
  props: PeriodFilterProps<Period>,
) {
  const Filter = usePlugin().components?.PeriodFilter;
  return Filter ? (
    <Filter {...(props as unknown as PeriodFilterProps)} />
  ) : null;
}

/**
 * Where the plugin is set up in the dashboard: in this project (its
 * `plugin.setup` widget and settings there), or its settings outside one.
 */
export function setupHref(ctx: Where): string {
  return ctx.projectId === undefined
    ? `/settings/plugins/${ctx.pluginId}?tab=settings`
    : `/settings/plugins/${ctx.pluginId}/projects/${ctx.projectId}`;
}

/** A failed call to a plugin route, with the status and the server's detail. */
export class PluginApiError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

function readBody(response: Response): Promise<unknown> {
  const type = response.headers.get('content-type') ?? '';
  return type.includes('application/json') ? response.json() : response.text();
}

async function call(url: string, init?: RequestInit): Promise<unknown> {
  const response = await fetch(url, { credentials: 'same-origin', ...init });
  if (!response.ok) {
    const body = await readBody(response).catch(() => null);
    const detail =
      typeof body === 'object' && body && 'detail' in body
        ? String(body.detail)
        : String(body ?? '');
    throw new PluginApiError(response.status, detail || response.statusText);
  }
  return response.status === 204 ? null : readBody(response);
}

/** Reads one of the plugin's own routes, cached like every dashboard query. */
export function usePluginApi<T = unknown>(
  path: string,
  search?: Search,
): UseQueryResult<T> {
  const ctx = usePlugin();
  return useQuery({
    queryKey: pluginKeys.route(ctx, path, search),
    queryFn: () => call(pluginUrl(ctx, path, search)) as Promise<T>,
  });
}

/**
 * The plugin's settings, typed from rustrak.config.ts: in a project, the
 * project's values, then the instance's, then the defaults. Secrets are left
 * out; only the plugin's server reads them.
 */
export function usePluginSettings(): UseQueryResult<PluginSettings> {
  const ctx = usePlugin();
  return useQuery({
    // The same entry the settings form edits: the whole view, read here
    // for what the plugin resolves.
    queryKey: pluginKeys.settings(ctx.pluginId, ctx.projectId),
    queryFn: () =>
      call(settingsUrl(ctx)) as Promise<{ resolved: PluginSettings }>,
    select: (view) => view.resolved,
  });
}

/**
 * Reads one of the plugin's routes outside render, from an event handler or
 * a query bar's suggestions, cached like `usePluginApi`.
 */
export function usePluginFetch(): <T = unknown>(
  path: string,
  search?: Search,
) => Promise<T> {
  const ctx = usePlugin();
  const client = useQueryClient();
  return <T,>(path: string, search?: Search) =>
    client.fetchQuery({
      queryKey: pluginKeys.route(ctx, path, search),
      queryFn: () => call(pluginUrl(ctx, path, search)) as Promise<T>,
      staleTime: 30_000,
    });
}

/**
 * Reads Rustrak's own API (`/api/projects/3`, an issue's events) as the
 * person looking: their session, their permissions, the dashboard's cache.
 */
export function useRustrakApi<T = unknown>(
  path: string,
  options: { enabled?: boolean } = {},
): UseQueryResult<T> {
  return useQuery({
    queryKey: ['rustrak', path],
    queryFn: () => call(path) as Promise<T>,
    enabled: options.enabled,
  });
}

/** Writes to one of the plugin's routes; the plugin's reads refresh after it. */
export function usePluginMutation<TBody = unknown, TResult = unknown>(
  path: string,
  method: 'POST' | 'PUT' | 'PATCH' | 'DELETE' = 'POST',
): UseMutationResult<TResult, PluginApiError, TBody> {
  const ctx = usePlugin();
  const client = useQueryClient();
  return useMutation({
    mutationFn: (body: TBody) =>
      call(pluginUrl(ctx, path), {
        method,
        headers: {
          'content-type':
            typeof body === 'string' ? 'text/plain' : 'application/json',
        },
        body: typeof body === 'string' ? body : JSON.stringify(body),
      }) as Promise<TResult>,
    onSuccess: () =>
      client.invalidateQueries({ queryKey: pluginKeys.all(ctx.pluginId) }),
  });
}
