import { describe, expect, it } from 'vitest';
import { resolveSetupSnippet } from './setup-snippet';

const DSN = 'https://key@rustrak.test/1';

describe('resolveSetupSnippet', () => {
  it('renders the platform snippet with the project DSN filled in', () => {
    const setup = resolveSetupSnippet('javascript-react', DSN);

    expect(setup.codeExample).toContain(DSN);
  });

  it('falls back to a browser example when no platform is set', () => {
    // A project that has never ingested an event has no platform, and the
    // generic browser snippet is the one that fits every such case.
    const setup = resolveSetupSnippet(null, DSN);

    expect(setup.codeExample).toContain('@sentry/browser');
    expect(setup.codeExample).toContain(DSN);
  });

  /**
   * The rule the whole function exists for. Snippet coverage is partial by
   * design and the platforms it misses are mostly console and native ones
   * (unreal, playstation, xbox, native...), where a `@sentry/browser` example
   * is actively misleading. Sentry is stricter still, rendering an explicit
   * "currently unavailable" rather than substituting another platform's.
   */
  it('offers no example for a platform it has no snippet for', () => {
    const setup = resolveSetupSnippet('playstation', DSN);

    expect(setup.codeExample).toBeUndefined();
    expect(setup.snippet).toBeUndefined();
  });

  it('defaults the language when the snippet does not name one', () => {
    expect(resolveSetupSnippet(null, DSN).codeLanguage).toBe('javascript');
  });
});
