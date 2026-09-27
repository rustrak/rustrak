import type { User } from '@rustrak/client';
import type { Messages } from 'use-intl';
import { describe, expect, it, vi } from 'vitest';
import type { CurrentUser } from '@/shared/api/session';
import { createIntlStore } from './intl';

/**
 * What `getRequestConfig` used to decide per request, decided once per reader.
 *
 * The interesting cases are the ones a static bundle introduces: nobody is
 * signed in yet when the login page renders, and the reader can change their
 * language from inside the running application without a server round trip to
 * re-render them.
 */

function user(overrides: Partial<User> = {}): CurrentUser {
  return {
    state: 'authenticated',
    user: { id: 1, email: 'alice@example.com', ...overrides } as User,
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

  it('has no snapshot before it is resolved, and says so', () => {
    const store = createIntlStore(deps());

    expect(() => store.snapshot()).toThrow(/have not been resolved/);
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

  // The first paint waits on both, so the catalogue must not wait on the
  // session: the browser's language is the likely answer, and the account
  // only changes it for a reader whose preference differs.
  it('starts loading the browser catalogue before the session answers', async () => {
    let answer: (session: CurrentUser) => void = () => {};
    const loadMessages = vi.fn(async () => messages);
    const store = createIntlStore(
      deps({
        readSession: () => new Promise((resolve) => (answer = resolve)),
        browserLanguages: () => ['fr-FR'],
        loadMessages,
      }),
    );

    const pending = store.ensure();
    expect(loadMessages).toHaveBeenCalledWith('fr');

    answer(user({ language: 'fr' }));
    expect((await pending).locale).toBe('fr');
    expect(loadMessages).toHaveBeenCalledTimes(1);
  });

  it('loads the account language when it differs from the browser', async () => {
    const loadMessages = vi.fn(async () => messages);
    const store = createIntlStore(
      deps({
        readSession: async () => user({ language: 'es' }),
        browserLanguages: () => ['fr-FR'],
        loadMessages,
      }),
    );

    expect((await store.ensure()).locale).toBe('es');
    expect(loadMessages).toHaveBeenLastCalledWith('es');
  });
});
