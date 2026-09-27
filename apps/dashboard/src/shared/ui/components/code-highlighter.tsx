import { lazy, Suspense } from 'react';
import type { PrismHighlighterProps } from './prism-highlighter';

/**
 * Highlighted code, with the highlighter out of every route's first payload.
 *
 * Until the chunk lands, and for good if it never does, the code renders as
 * plain text with the same box, so the text is readable at once and nothing
 * moves when the colours arrive.
 */
const PrismHighlighter = lazy(() =>
  import('./prism-highlighter').catch(() => ({ default: PlainCode })),
);

export function CodeHighlighter(props: PrismHighlighterProps) {
  return (
    <Suspense fallback={<PlainCode {...props} />}>
      <PrismHighlighter {...props} />
    </Suspense>
  );
}

function PlainCode({ code, customStyle, codeTagProps }: PrismHighlighterProps) {
  return (
    <pre style={customStyle}>
      <code {...codeTagProps}>{code}</code>
    </pre>
  );
}
