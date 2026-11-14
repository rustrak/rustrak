import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { brotliCompressSync, constants, gzipSync } from 'node:zlib';
import tailwindcss from '@tailwindcss/vite';
import { tanstackRouter } from '@tanstack/router-plugin/vite';
import viteReact from '@vitejs/plugin-react';
import { defineConfig, loadEnv, type Plugin } from 'vite';

/**
 * Writes `.br` and `.gz` beside every text asset, at the highest levels.
 *
 * The server's `Files` mount picks them by `Accept-Encoding`
 * (`try_compressed` in `routes/dashboard.rs`), so it never compresses an
 * asset itself: brotli 11 is smaller than the quality-3 brotli the `Compress`
 * middleware can afford per request, and costs the server no CPU at all.
 * Done once here, at build time, where the time is free.
 */
function precompress(): Plugin {
  let outDir = 'dist';
  return {
    name: 'rustrak:precompress',
    apply: 'build',
    configResolved(config) {
      outDir = config.build.outDir;
    },
    closeBundle() {
      const assets = join(outDir, 'assets');
      for (const name of readdirSync(assets)) {
        if (!/\.(js|css|svg|json)$/.test(name)) continue;
        const path = join(assets, name);
        const bytes = readFileSync(path);
        // Below a packet, the encoding header costs more than it saves.
        if (bytes.length < 1024) continue;
        writeFileSync(
          `${path}.br`,
          brotliCompressSync(bytes, {
            params: { [constants.BROTLI_PARAM_QUALITY]: 11 },
          }),
        );
        writeFileSync(`${path}.gz`, gzipSync(bytes, { level: 9 }));
      }
    },
  };
}

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
  const apiPrefixes = [
    '/api',
    '/auth',
    '/health',
    '/docs',
    '/api-docs',
    '/metrics',
    '/mcp',
    '/.well-known',
  ];

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
      tanstackRouter({
        target: 'react',
        autoCodeSplitting: true,
        codeSplittingOptions: {
          // The plugin's default splits `component` but leaves
          // `pendingComponent` in the route file, and every route's skeleton
          // is its page rendered without data. So every page, and everything
          // it imports (recharts included), sat in the entry chunk. One chunk
          // for both: the router already waits for `component`'s chunk before
          // it renders, so the skeleton arrives with it and costs no wait.
          defaultBehavior: [
            ['component', 'pendingComponent'],
            ['errorComponent'],
            ['notFoundComponent'],
          ],
        },
      }),
      viteReact(),
      precompress(),
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
      // Never inline an SVG. Vite would turn every platform logo under 4 KB
      // into a base64 string, and `platformicons` names ~400 of them: 600 KB
      // of JS on the first load for the handful a page actually shows. As
      // files they are fetched one by one, only when drawn, and cached forever.
      assetsInlineLimit: (file) => (file.endsWith('.svg') ? false : undefined),
    },
  };
});
