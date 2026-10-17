import { Loader2 } from 'lucide-react';
import type { SyntaxHighlighter } from '@/shared/ui/hooks/use-syntax-highlighter';

/**
 * The example, highlighted where the highlighter arrived.
 *
 * Three states, and the third is why `failed` exists: a chunk that never
 * loaded must fall back to readable plain text, not shimmer forever.
 */
export function CodeBlock({
  code,
  language,
  highlighter: { Highlighter, style, failed, background },
}: {
  code: string;
  language: string;
  highlighter: SyntaxHighlighter;
}) {
  if (Highlighter && style) {
    return (
      <Highlighter
        language={language}
        style={style}
        customStyle={{
          margin: 0,
          padding: '1rem',
          fontSize: '0.75rem',
          background,
        }}
      >
        {code}
      </Highlighter>
    );
  }

  if (failed) {
    return <pre className="overflow-x-auto p-4 font-mono text-xs">{code}</pre>;
  }

  return (
    <div className="flex h-24 animate-pulse items-center justify-center bg-muted p-4">
      <Loader2 className="size-4 animate-spin text-muted-foreground" />
    </div>
  );
}
