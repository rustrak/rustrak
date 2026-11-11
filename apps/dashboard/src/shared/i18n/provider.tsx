import { useEffect, useSyncExternalStore } from 'react';
import { IntlProvider } from 'use-intl';
import { intl } from './intl';

/**
 * The messages, for the whole application: one lazily-imported catalogue the
 * browser fetches once. It re-renders when `intl.reload()` publishes a new
 * language.
 */
export function Messages({ children }: { children: React.ReactNode }) {
  const snapshot = useSyncExternalStore(
    intl.subscribe,
    intl.snapshot,
    intl.snapshot,
  );

  // `<html lang>` was rendered per request by the server. Nothing but this
  // sets it on a static shell, and it is what a screen reader picks a voice
  // from — the one piece of the document Tailwind cannot express.
  useEffect(() => {
    document.documentElement.lang = snapshot.locale;
  }, [snapshot.locale]);

  return (
    <IntlProvider
      locale={snapshot.locale}
      messages={snapshot.messages}
      timeZone={snapshot.timeZone}
      formats={snapshot.formats}
      now={snapshot.now}
    >
      {children}
    </IntlProvider>
  );
}
