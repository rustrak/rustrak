import { SpinnerIcon } from '@rustrak/ui';
import { codeTheme } from '@/shared/ui/components/code-theme';
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
  highlighter: { Highlighter, failed },
}: {
  code: string;
  language: string;
  highlighter: SyntaxHighlighter;
}) {
  if (Highlighter) {
    return (
      <Highlighter
        language={language}
        style={codeTheme}
        customStyle={{
          margin: 0,
          padding: '1rem',
          fontSize: '0.75rem',
          // A token, so the block follows the theme switch with the page.
          background: 'var(--surface-code)',
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
    <div className="flex h-24 animate-pulse items-center justify-center bg-surface-hover p-4">
      <SpinnerIcon className="size-4 animate-spin text-fg-subtle" />
    </div>
  );
}
