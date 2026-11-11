import '@fontsource-variable/inter/opsz.css';
import '@fontsource-variable/geist-mono';
import './styles.css';

import { QueryClientProvider } from '@tanstack/react-query';
import { ReactQueryDevtools } from '@tanstack/react-query-devtools';
import { RouterProvider } from '@tanstack/react-router';
import { ThemeProvider } from 'next-themes';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import {
  clearOnUserChange,
  createQueryClient,
  refreshNowOnData,
} from '@/shared/api/query';
import { session } from '@/shared/api/session';
import { intl } from '@/shared/i18n/intl';
import { Messages } from '@/shared/i18n/provider';
import { DialogHost } from '@/shared/ui/components/dialog-host';
import { Toaster } from '@/shared/ui/components/toaster';
import { createAppRouter } from './router';

const queryClient = createQueryClient();
// Both subscriptions last as long as the app, so nothing unsubscribes them.
clearOnUserChange(queryClient, session);
refreshNowOnData(queryClient, intl);
const router = createAppRouter(queryClient);

/**
 * Nothing renders until we know who is asking and in what language, so the
 * first paint is the finished page in the reader's own language.
 *
 * `intl.ensure()` reads the session itself, and both are memoised stores, so
 * the guards that run next find the answers already there. It resolves for an
 * unreachable server too (falling back to the browser's language); it only
 * rejects for a catalogue that would not load, and English then beats a blank
 * page.
 */
async function bootstrap() {
  await intl.ensure().catch((error: unknown) => console.error(error));

  const container = document.getElementById('root');
  if (!container) throw new Error('index.html is missing #root');

  createRoot(container).render(
    <StrictMode>
      {/* `data-theme` drives `@rustrak/ui`; `.dark` the two comboboxes still
          styled with `dark:` (platform picker, time zone). */}
      <ThemeProvider
        attribute={['class', 'data-theme']}
        defaultTheme="dark"
        enableSystem
        disableTransitionOnChange
      >
        {/* Above the router, like the toaster: the cache must outlive every
            navigation, or going back would refetch what was just on screen. */}
        <QueryClientProvider client={queryClient}>
          <Messages>
            <DialogHost>
              <RouterProvider router={router} />
            </DialogHost>
            <Toaster />
          </Messages>
          {/* Dropped from production builds by the package itself. */}
          <ReactQueryDevtools buttonPosition="bottom-left" />
        </QueryClientProvider>
      </ThemeProvider>
    </StrictMode>,
  );
}

void bootstrap();
