import '@fontsource-variable/geist';
import '@fontsource-variable/geist-mono';
import './styles.css';

import { QueryClientProvider } from '@tanstack/react-query';
import { RouterProvider } from '@tanstack/react-router';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { queryClient } from '@/shared/api/query-client';
import { session } from '@/shared/api/session';
import { intl } from '@/shared/i18n/intl';
import { Messages } from '@/shared/i18n/provider';
import { ThemeProvider } from '@/shared/ui/components/theme-provider';
import { Toaster } from '@/shared/ui/components/toaster';
import { connectRouter, createAppRouter } from './router';

const router = createAppRouter();
connectRouter(router);

/**
 * Nothing renders until we know who is asking and in what language.
 *
 * That is not a performance oversight, it is the same order Next kept: the
 * server resolved the session and the message catalogue before it emitted a
 * byte of HTML, so the first thing a reader ever saw was the finished page in
 * their own language. Painting an English shell first and swapping it a
 * moment later would be new behaviour, and the visible kind.
 *
 * Both reads are memoised stores, so the guards that run immediately after
 * this find the answers already there and issue no second request.
 *
 * A failure here is not fatal. `intl.ensure()` resolves for an unreachable
 * server too — it falls back to the browser's language — so the only way this
 * rejects is a catalogue that would not load, and then English is still a
 * better answer than a blank page.
 */
async function bootstrap() {
  await Promise.all([session.ensure(), intl.ensure()]).catch(() => undefined);

  const container = document.getElementById('root');
  if (!container) throw new Error('index.html is missing #root');

  createRoot(container).render(
    <StrictMode>
      <ThemeProvider
        attribute="class"
        defaultTheme="dark"
        enableSystem
        disableTransitionOnChange
      >
        <QueryClientProvider client={queryClient}>
          <Messages>
            <RouterProvider router={router} />
            <Toaster />
          </Messages>
        </QueryClientProvider>
      </ThemeProvider>
    </StrictMode>,
  );
}

void bootstrap();
