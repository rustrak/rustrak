import {
  PLATFORM_DOCS,
  PLATFORM_SNIPPETS,
  renderSnippet,
} from './platform-snippets';

/** The generic example for a project that has never named a platform. */
const browserExample = (
  dsn: string,
) => `import * as Sentry from "@sentry/browser";

Sentry.init({
  dsn: "${dsn}",
});`;

export interface SetupSnippet {
  snippet: (typeof PLATFORM_SNIPPETS)[string] | undefined;
  docsUrl: string | undefined;
  /** Absent when this platform has no example worth showing. */
  codeExample: string | undefined;
  codeLanguage: string;
}

/**
 * The setup instructions for one project, following its own platform.
 *
 * Only a project with no platform at all (never set, no event ingested yet)
 * gets the generic browser example. A project that **has** a platform but no
 * snippet must never get it: snippet coverage is partial by design, and the
 * platforms it misses are mostly console and native ones (unreal, playstation,
 * xbox, native...) where a `@sentry/browser` example is actively misleading.
 * Sentry does the same, and more strictly: `sdkDocumentation.tsx` renders an
 * explicit "currently unavailable" rather than substituting another
 * platform's snippet.
 */
export function resolveSetupSnippet(
  platform: string | null | undefined,
  dsn: string,
): SetupSnippet {
  const snippet = platform ? PLATFORM_SNIPPETS[platform] : undefined;

  const codeExample = snippet
    ? renderSnippet(snippet.configure, dsn)
    : platform
      ? undefined
      : browserExample(dsn);

  return {
    snippet,
    docsUrl: platform ? PLATFORM_DOCS[platform] : undefined,
    codeExample,
    codeLanguage: snippet?.language ?? 'javascript',
  };
}
