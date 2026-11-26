import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import * as React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import {
  PeriodFilter,
  PluginContext,
  PluginLink,
  PluginViews,
  publicUrl,
  usePluginFetch,
  usePluginPermissions,
  usePluginSearch,
  useRustrakApi,
} from '../src/ui/index.tsx';

/** What the dashboard always lends a plugin. */
const host = {
  messages: {},
  navigate: () => {},
  can: () => false,
};

const inPlugin = (value: Record<string, unknown>, node: React.ReactElement) =>
  renderToStaticMarkup(
    React.createElement(
      PluginContext.Provider,
      { value: { ...host, pluginId: 'hello', ...value } },
      node,
    ),
  );

const link = (value: Record<string, unknown>, props: Record<string, unknown>) =>
  inPlugin(
    value,
    React.createElement(PluginLink, { children: 'x', ...props } as never),
  );

describe('PluginLink', () => {
  it("points at the plugin's own pages, in the project or outside any", () => {
    expect(link({ projectId: 3 }, { to: '' })).toBe(
      '<a href="/projects/3/plugins/hello">x</a>',
    );
    expect(link({}, { to: '' })).toBe('<a href="/plugins/hello">x</a>');
  });

  it("carries a page's query along, leaving out what is empty", () => {
    expect(
      link(
        { projectId: 3 },
        { to: 'visitors', search: { country: 'ES', page: '' } },
      ),
    ).toBe('<a href="/projects/3/plugins/hello/visitors?country=ES">x</a>');
  });
});

describe('usePluginPermissions', () => {
  it("asks the dashboard about the plugin's own key, in its project", () => {
    const asked: unknown[] = [];
    const Probe = () => {
      const { can } = usePluginPermissions();
      return React.createElement('span', null, String(can('hits', 'delete')));
    };
    const html = renderToStaticMarkup(
      React.createElement(
        PluginContext.Provider,
        {
          value: {
            ...host,
            pluginId: 'hello',
            projectId: 3,
            can: (...args: unknown[]) => {
              asked.push(args);
              return true;
            },
          },
        },
        React.createElement(Probe),
      ),
    );
    expect(html).toBe('<span>true</span>');
    expect(asked).toEqual([['hello:hits', 'delete', 3]]);
  });
});

describe('publicUrl', () => {
  it('is where anyone loads one of its public files, on this Rustrak', () => {
    const original = globalThis.window;
    globalThis.window = {
      location: { origin: 'https://rustrak.example.com' },
    } as never;
    try {
      expect(publicUrl({ pluginId: 'web-traffic' }, 'script.js')).toBe(
        'https://rustrak.example.com/public/web-traffic/script.js',
      );
    } finally {
      globalThis.window = original;
    }
  });
});

describe('useRustrakApi', () => {
  it("reads Rustrak's own API as the reader, sharing the dashboard's cache", async () => {
    const asked: string[] = [];
    const original = globalThis.fetch;
    globalThis.fetch = (async (url: string) => {
      asked.push(url);
      return new Response(JSON.stringify({ id: 3, sentry_key: 'k' }), {
        headers: { 'content-type': 'application/json' },
      });
    }) as typeof fetch;
    const client = new QueryClient();
    const Probe = () => {
      const project = useRustrakApi<{ sentry_key: string }>('/api/projects/3');
      return React.createElement('span', null, project.data?.sentry_key ?? '…');
    };
    const draw = () =>
      renderToStaticMarkup(
        React.createElement(
          QueryClientProvider,
          { client },
          React.createElement(
            PluginContext.Provider,
            { value: { ...host, pluginId: 'web-traffic', projectId: 3 } },
            React.createElement(Probe),
          ),
        ),
      );
    try {
      // Server rendering starts no read: the hook only registers it, with
      // its own fetch, which runs here.
      expect(draw()).toBe('<span>…</span>');
      await client
        .getQueryCache()
        .find({ queryKey: ['rustrak', '/api/projects/3'] })
        ?.fetch();
      expect(draw()).toBe('<span>k</span>');
      expect(asked).toEqual(['/api/projects/3']);
    } finally {
      globalThis.fetch = original;
    }
  });
});

describe("the page's address and the dashboard's own controls", () => {
  const render = (value: Record<string, unknown>, node: React.ReactElement) =>
    renderToStaticMarkup(
      React.createElement(
        PluginContext.Provider,
        { value: { ...host, pluginId: 'web-traffic', ...value } },
        node,
      ),
    );

  it('reads the search and writes it merged, dropping what is emptied', () => {
    const written: Record<string, string>[] = [];
    let set: ((next: Record<string, string | undefined>) => void) | undefined;
    const Probe = () => {
      const [search, setSearch] = usePluginSearch();
      set = setSearch;
      return React.createElement('span', null, search.period);
    };
    const html = render(
      {
        search: { period: '7d', country: 'ES' },
        setSearch: (next: Record<string, string>) => written.push(next),
      },
      React.createElement(Probe),
    );
    expect(html).toBe('<span>7d</span>');
    set?.({ country: undefined, browser: 'Firefox' });
    expect(written).toEqual([{ period: '7d', browser: 'Firefox' }]);
  });

  it("draws the dashboard's views menu and window, and nothing without a host", () => {
    const components = {
      Views: ({ search }: { search: Record<string, string> }) =>
        React.createElement('i', null, `views:${search.period}`),
      PeriodFilter: ({ value }: { value: string }) =>
        React.createElement('b', null, `period:${value}`),
    };
    const page = React.createElement(
      React.Fragment,
      null,
      React.createElement(PluginViews),
      React.createElement(PeriodFilter, { value: '30d', onChange: () => {} }),
    );
    expect(render({ search: { period: '7d' }, components }, page)).toBe(
      '<i>views:7d</i><b>period:30d</b>',
    );
    expect(render({}, page)).toBe('');
  });
});

describe('usePluginFetch', () => {
  it("reads a plugin route outside render, through the dashboard's cache", async () => {
    const asked: string[] = [];
    const original = globalThis.fetch;
    globalThis.fetch = (async (url: string) => {
      asked.push(url);
      return new Response('[{"value":"ES"}]', {
        headers: { 'content-type': 'application/json' },
      });
    }) as typeof fetch;
    let read: ReturnType<typeof usePluginFetch> | undefined;
    const Probe = () => {
      read = usePluginFetch();
      return null;
    };
    try {
      renderToStaticMarkup(
        React.createElement(
          QueryClientProvider,
          { client: new QueryClient() },
          React.createElement(
            PluginContext.Provider,
            { value: { ...host, pluginId: 'web-traffic', projectId: 3 } },
            React.createElement(Probe),
          ),
        ),
      );
      expect(await read!('/top', { by: 'country' })).toEqual([{ value: 'ES' }]);
      await read!('/top', { by: 'country' });
      expect(asked).toEqual([
        '/api/projects/3/plugins/web-traffic/top?by=country',
      ]);
    } finally {
      globalThis.fetch = original;
    }
  });
});
