import { fileURLToPath } from 'node:url';
import tailwindcss from '@tailwindcss/vite';
import { tanstackRouter } from '@tanstack/router-plugin/vite';
import viteReact from '@vitejs/plugin-react';
import { defineConfig, loadEnv } from 'vite';

/**
 * The dashboard is a pure SPA. It is compiled to static files and the Rust
 * server hands them out; there is no Node process in production.
 *
 * That means the two environments have to agree on one thing: the browser
 * always talks to its own origin. In production that is literally true -- the
 * same Actix process answers `/` and `/api`. In development Vite owns the
 * origin and proxies the API prefixes across to the server, which keeps the
 * session cookie first-party and CORS out of the picture entirely.
 */
export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '');
  const server = env.RUSTRAK_SERVER_URL || 'http://127.0.0.1:8080';

  /**
   * Everything the Rust server owns. Every other path belongs to the router,
   * in development and in production alike -- `routes::dashboard` in the
   * server keeps the same list, and the two must not drift.
   */
  const apiPrefixes = ['/api', '/auth', '/health', '/docs', '/api-docs'];

  const proxy = Object.fromEntries(
    apiPrefixes.map((prefix) => [
      prefix,
      {
        target: server,
        changeOrigin: true,
        // The server sets `Secure` on the session cookie whenever SSL_PROXY is
        // on. Over plain http on localhost the browser would drop it and the
        // dashboard would look permanently logged out, so the dev proxy takes
        // the attribute back off. Only ever runs in `vite dev`.
        cookieDomainRewrite: '',
        configure: (proxyServer: {
          on: (
            event: string,
            listener: (proxyRes: {
              headers: Record<string, string | string[] | undefined>;
            }) => void,
          ) => void;
        }) => {
          proxyServer.on('proxyRes', (proxyRes) => {
            const cookies = proxyRes.headers['set-cookie'];
            if (Array.isArray(cookies)) {
              proxyRes.headers['set-cookie'] = cookies.map((cookie) =>
                cookie.replace(/;\s*Secure/gi, ''),
              );
            }
          });
        },
      },
    ]),
  );

  return {
    // Served from the root of the server's origin. A sub-path deployment would
    // change this *and* the router's basepath in `src/router.tsx`.
    base: '/',
    resolve: {
      alias: {
        '@': fileURLToPath(new URL('./src', import.meta.url)),
      },
    },
    plugins: [
      tailwindcss(),
      tanstackRouter({ target: 'react', autoCodeSplitting: true }),
      viteReact(),
    ],
    server: {
      port: 3000,
      proxy,
    },
    preview: {
      port: 3000,
      proxy,
    },
    build: {
      outDir: 'dist',
      emptyOutDir: true,
      // Vite fingerprints everything under `assets/`, which is what lets the
      // server mark that one directory `immutable` and nothing else.
      assetsDir: 'assets',
      // `platformicons` imports all ~450 of its SVGs from one module, and each
      // is under the 4 KB inline limit, so by default every icon became a
      // base64 string in the chunk every screen loads. As files, the browser
      // fetches only the icons a page draws, and caches them.
      assetsInlineLimit: (file) =>
        file.includes('/platformicons/') ? false : undefined,
      rolldownOptions: {
        output: {
          // The framework in chunks of its own. Left to itself, Rolldown can
          // place React inside whichever shared chunk it builds first (it
          // once landed in recharts', which put the chart library in the
          // entry), and a deploy that changes no framework code still
          // changes those chunks' hashes.
          codeSplitting: {
            groups: [
              {
                name: 'react',
                test: /node_modules[\\/](react|react-dom|scheduler)[\\/]/,
                priority: 30,
              },
              {
                name: 'tanstack',
                test: /node_modules[\\/]@tanstack[\\/](react-router|router-core|history|react-store|store|query-core|react-query)[\\/]/,
                priority: 20,
              },
            ],
          },
        },
      },
    },
  };
});
