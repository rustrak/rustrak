import { useEffect, useState } from 'react';

type HighlighterComponent = typeof import('react-syntax-highlighter').Prism;
type HighlighterStyle = Record<string, React.CSSProperties>;

export interface SyntaxHighlighter {
  /** `null` until the chunk arrives, and forever if it never does. */
  Highlighter: HighlighterComponent | null;
  style: HighlighterStyle | null;
  /** The chunk failed. Render the code as plain text rather than spinning. */
  failed: boolean;
  /**
   * The canvas the chosen style expects, as a literal.
   *
   * Paired with `style` here rather than read off the theme at the call site:
   * the two are picked by the same `isDark`, and letting them be decided in
   * two places is how a dark block ends up on a white card mid theme-switch.
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
export function useSyntaxHighlighter(isDark: boolean): SyntaxHighlighter {
  const [Highlighter, setHighlighter] = useState<HighlighterComponent | null>(
    null,
  );
  const [style, setStyle] = useState<HighlighterStyle | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;

    Promise.all([
      import('react-syntax-highlighter').then((mod) => mod.Prism),
      import('react-syntax-highlighter/dist/esm/styles/prism'),
    ])
      .then(([component, styles]) => {
        if (cancelled) return;
        setHighlighter(() => component);
        setStyle((isDark ? styles.vscDarkPlus : styles.vs) as HighlighterStyle);
      })
      .catch(() => {
        if (!cancelled) setFailed(true);
      });

    return () => {
      cancelled = true;
    };
  }, [isDark]);

  return {
    Highlighter,
    style,
    failed,
    background: isDark ? '#1e1e1e' : '#ffffff',
  };
}
