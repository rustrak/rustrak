import type { UserResponse } from '@rustrak/client';
import type { Messages } from 'use-intl';
import { describe, expect, it, vi } from 'vitest';
import type { CurrentUser } from '@/shared/api/session';
import { createSessionStore } from '@/shared/api/session';
import { createIntlStore, reloadOnUserChange } from './intl';

/**
 * What `getRequestConfig` used to decide per request, decided once per reader.
 *
 * The interesting cases are the ones a static bundle introduces: nobody is
 * signed in yet when the login page renders, and the reader can change their
 * language from inside the running application without a server round trip to
 * re-render them.
 */

function user(overrides: Partial<UserResponse> = {}): CurrentUser {
  return {
    state: 'authenticated',
    user: { id: 1, email: 'alice@example.com', ...overrides } as UserResponse,
  };
}

const messages = { app: { title: 'Rustrak' } } as unknown as Messages;

function deps(overrides: Partial<Parameters<typeof createIntlStore>[0]> = {}) {
  return {
    readSession: async (): Promise<CurrentUser> => ({ state: 'anonymous' }),
    browserLanguages: () => ['en'],
    loadMessages: async () => messages,
    ...overrides,
  };
}

describe('createIntlStore', () => {
  it('takes the language and the zone off the account', async () => {
    const store = createIntlStore(
      deps({
        readSession: async () =>
          user({ language: 'fr', timezone: 'Europe/Madrid' }),
      }),
    );

    const snapshot = await store.ensure();

    expect(snapshot.locale).toBe('fr');
    expect(snapshot.timeZone).toBe('Europe/Madrid');
  });

  // The login and the invitation render before anyone is anyone. There is no
  // row to read a preference off, so the browser's list is all there is.
  it('falls back to the browser when nobody is signed in', async () => {
    const store = createIntlStore(
      deps({
        readSession: async () => ({ state: 'anonymous' }),
        browserLanguages: () => ['es-ES', 'en'],
      }),
    );

    expect((await store.ensure()).locale).toBe('es');
  });

  // An unreachable server must still render "the server did not answer", in
  // some language, rather than throwing before anything paints.
  it('renders in the browser language when the session is unreachable', async () => {
    const store = createIntlStore(
      deps({
        readSession: async () => ({
          state: 'unavailable',
          error: { kind: 'network', message: 'no', reason: 'unreachable' },
        }),
        browserLanguages: () => ['ro'],
      }),
    );

    expect((await store.ensure()).locale).toBe('ro');
  });

  it('loads one catalogue however many callers ask for it', async () => {
    const loadMessages = vi.fn(async () => messages);
    const store = createIntlStore(deps({ loadMessages }));

    await Promise.all([store.ensure(), store.ensure(), store.ensure()]);

    expect(loadMessages).toHaveBeenCalledTimes(1);
  });

  // Changing the language on /settings/account used to be a server re-render.
  // Here it is this: write the preference, then re-resolve.
  it('re-resolves on reload and tells the provider', async () => {
    let language = 'en';
    const listener = vi.fn();
    const store = createIntlStore(
      deps({ readSession: async () => user({ language }) }),
    );
    store.subscribe(listener);

    expect((await store.ensure()).locale).toBe('en');

    language = 'zh';
    expect((await store.reload()).locale).toBe('zh');
    expect(store.snapshot().locale).toBe('zh');
    expect(listener).toHaveBeenCalledTimes(2);
  });

  it('carries the named formats for the resolved zone', async () => {
    const store = createIntlStore(
      deps({ readSession: async () => user({ timezone: 'Europe/Madrid' }) }),
    );

    const { formats } = await store.ensure();

    // Sentry's rule: no zone label when the reader has one of their own.
    expect(formats.dateTime?.dateTime).toMatchObject({ timeStyle: 'short' });
  });

  it("puts the reader's clock into the formats, and leaves it out when unset", async () => {
    const chosen = createIntlStore(
      deps({ readSession: async () => user({ clock_24h: true }) }),
    );
    const unset = createIntlStore(
      deps({ readSession: async () => user({ clock_24h: null }) }),
    );

    expect((await chosen.ensure()).formats.dateTime?.logLine).toMatchObject({
      hourCycle: 'h23',
    });
    expect((await unset.ensure()).formats.dateTime?.logLine).not.toHaveProperty(
      'hourCycle',
    );
  });

  it('has no snapshot before it is resolved, and says so', () => {
    const store = createIntlStore(deps());

    expect(() => store.snapshot()).toThrow();
  });

  it('does not remember a failed resolve', async () => {
    const readSession = vi
      .fn<() => Promise<CurrentUser>>()
      .mockRejectedValueOnce(new Error('offline'))
      .mockResolvedValue(user({ language: 'es' }));
    const store = createIntlStore(deps({ readSession }));

    await expect(store.ensure()).rejects.toThrow('offline');
    expect((await store.ensure()).locale).toBe('es');
  });
});

describe('overlapping resolves', () => {
  // A reader signs in with French and out again before the French catalogue
  // has arrived. The later resolve (anonymous, English) lands first; the
  // earlier one must not overwrite it when it finally does.
  it('keeps the latest resolve when an older one finishes after it', async () => {
    let releaseFrench: (messages: Messages) => void = () => {};
    const french = new Promise<Messages>((resolve) => {
      releaseFrench = resolve;
    });
    let current: CurrentUser = user({ language: 'fr', clock_24h: true });
    const store = createIntlStore(
      deps({
        readSession: async () => current,
        loadMessages: (locale) =>
          locale === 'fr' ? french : Promise.resolve(messages),
      }),
    );
    const listener = vi.fn();
    store.subscribe(listener);

    const signedIn = store.reload();
    current = { state: 'anonymous' };
    const signedOut = await store.reload();
    releaseFrench(messages);

    expect(signedOut.locale).toBe('en');
    expect(await signedIn).toBe(signedOut);
    expect(store.snapshot().locale).toBe('en');
    expect(store.snapshot().formats.dateTime?.logLine).not.toHaveProperty(
      'hourCycle',
    );
    expect(listener).toHaveBeenCalledTimes(1);
  });
});

describe('reloadOnUserChange', () => {
  // The login page renders before anyone is signed in, so the snapshot holds
  // the browser's language, UTC and the language's clock. Password sign-in
  // stays inside the SPA, so without this the reader's saved language, zone
  // and clock wait for a full reload.
  it('re-resolves for the reader who just signed in', async () => {
    const session = createSessionStore(async () => ({ state: 'anonymous' }));
    const store = createIntlStore(
      deps({ readSession: () => session.ensure() }),
    );
    reloadOnUserChange(store, session);

    await store.ensure();
    expect(store.snapshot().formats.dateTime?.logLine).not.toHaveProperty(
      'hourCycle',
    );

    session.set(user({ clock_24h: true }));
    await vi.waitFor(() =>
      expect(store.snapshot().formats.dateTime?.logLine).toMatchObject({
        hourCycle: 'h23',
      }),
    );
  });

  // The first answer is the one `ensure()` is already resolving: reloading
  // for it would resolve the same reader twice on every page load.
  it('leaves the first answer to ensure()', async () => {
    const session = createSessionStore(async () => user());
    const readSession = vi.fn(() => session.ensure());
    const store = createIntlStore(deps({ readSession }));
    reloadOnUserChange(store, session);

    await store.ensure();

    expect(readSession).toHaveBeenCalledTimes(1);
  });

  it('does not re-resolve when the same reader is published again', async () => {
    const session = createSessionStore(async () => user());
    const readSession = vi.fn(() => session.ensure());
    const store = createIntlStore(deps({ readSession }));
    reloadOnUserChange(store, session);
    await store.ensure();

    // A saved preference publishes the same reader; its control reloads.
    session.set(user({ language: 'fr' }));

    expect(readSession).toHaveBeenCalledTimes(1);
  });
});
