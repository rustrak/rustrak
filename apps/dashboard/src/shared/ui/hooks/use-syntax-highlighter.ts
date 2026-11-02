import { useEffect, useState } from 'react';
import { codeTheme } from '@/shared/ui/components/code-theme';

type HighlighterComponent = typeof import('react-syntax-highlighter').Prism;
type HighlighterStyle = Record<string, React.CSSProperties>;

export interface SyntaxHighlighter {
  /** `null` until the chunk arrives, and forever if it never does. */
  Highlighter: HighlighterComponent | null;
  style: HighlighterStyle | null;
  /** The chunk failed. Render the code as plain text rather than spinning. */
  failed: boolean;
  /**
   * The ground the code sits on. A token, like the theme's colours, so the
   * block follows the theme switch with the page.
   */
  background: string;
}

/**
 * The syntax highlighter, loaded lazily.
 *
 * It is far larger than the page's own code and the DSN above it stays useful
 * while it arrives, so it is never part of the first payload.
 *
 * A failed chunk is reported rather than swallowed: without `failed` the page
 * would show a loading shimmer for a load that already ended, and the reader
 * would wait for a snippet that is never coming.
 */
// `_isDark` is kept for the callers' signature: the theme is made of CSS
// variables now and follows the page on its own.
export function useSyntaxHighlighter(_isDark?: boolean): SyntaxHighlighter {
  const [Highlighter, setHighlighter] = useState<HighlighterComponent | null>(
    null,
  );
  const [style, setStyle] = useState<HighlighterStyle | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;

    import('react-syntax-highlighter')
      .then((mod) => {
        if (cancelled) return;
        setHighlighter(() => mod.Prism);
        setStyle(codeTheme);
      })
      .catch(() => {
        if (!cancelled) setFailed(true);
      });

    return () => {
      cancelled = true;
    };
  }, []);

  return {
    Highlighter,
    style,
    failed,
    background: 'var(--surface-code)',
  };
}
