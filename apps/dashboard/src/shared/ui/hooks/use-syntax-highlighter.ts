import { useEffect, useState } from 'react';

type HighlighterComponent = typeof import('react-syntax-highlighter').Prism;

export interface SyntaxHighlighter {
  /** `null` until the chunk arrives, and forever if it never does. */
  Highlighter: HighlighterComponent | null;
  /** The chunk failed. Render the code as plain text rather than spinning. */
  failed: boolean;
}

/**
 * The syntax highlighter, loaded lazily: it is far larger than the page's own
 * code and the page stays useful while it arrives.
 *
 * A failed chunk is reported rather than swallowed: without `failed` the page
 * would shimmer for a load that already ended.
 */
export function useSyntaxHighlighter(): SyntaxHighlighter {
  const [Highlighter, setHighlighter] = useState<HighlighterComponent | null>(
    null,
  );
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;

    import('react-syntax-highlighter')
      .then((mod) => {
        if (!cancelled) setHighlighter(() => mod.Prism);
      })
      .catch(() => {
        if (!cancelled) setFailed(true);
      });

    return () => {
      cancelled = true;
    };
  }, []);

  return { Highlighter, failed };
}
