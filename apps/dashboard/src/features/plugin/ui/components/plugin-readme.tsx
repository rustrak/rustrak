import type { ReactNode } from 'react';
import Markdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

/**
 * A plugin's README, as its page in the catalog. It comes from a package
 * someone else wrote: no raw HTML is rendered (react-markdown's default),
 * links open away from Rustrak, and images show their text instead of
 * fetching from wherever the README points.
 */
export function PluginReadme({ markdown }: { markdown: string }) {
  return (
    <div
      className="flex flex-col gap-3 px-6 py-5 text-value text-fg-secondary"
      data-testid="plugin-readme"
    >
      <Markdown
        remarkPlugins={[remarkGfm]}
        components={{
          h1: ({ children }) => (
            <h2 className="text-title text-fg">{children}</h2>
          ),
          h2: ({ children }) => (
            <h3 className="mt-3 text-section text-fg">{children}</h3>
          ),
          h3: ({ children }) => (
            <h4 className="mt-2 text-card-title text-fg">{children}</h4>
          ),
          p: ({ children }) => <p>{children}</p>,
          a: ({ href, children }) => (
            <a
              href={href}
              target="_blank"
              rel="noopener noreferrer"
              className="text-fg-brand underline-offset-2 hover:underline"
            >
              {children}
            </a>
          ),
          ul: ({ children }) => (
            <ul className="ml-5 list-disc space-y-1">{children}</ul>
          ),
          ol: ({ children }) => (
            <ol className="ml-5 list-decimal space-y-1">{children}</ol>
          ),
          code: ({ children, className }) =>
            className ? (
              <code className={className}>{children}</code>
            ) : (
              <code className="rounded bg-surface-hover px-1 py-0.5 font-mono text-mono-sm text-fg">
                {children}
              </code>
            ),
          pre: ({ children }) => (
            <pre className="overflow-x-auto rounded-lg border border-border-subtle bg-surface-hover p-3 font-mono text-mono-sm text-fg">
              {children}
            </pre>
          ),
          blockquote: ({ children }) => (
            <blockquote className="border-l-2 border-border-strong pl-3 text-fg-subtle">
              {children}
            </blockquote>
          ),
          table: ({ children }) => (
            <div className="overflow-x-auto">
              <table className="w-full border-collapse text-meta">
                {children}
              </table>
            </div>
          ),
          th: ({ children }) => (
            <th className="border-b border-border-subtle px-2 py-1.5 text-left text-fg">
              {children}
            </th>
          ),
          td: ({ children }) => (
            <td className="border-b border-border-divider px-2 py-1.5">
              {children}
            </td>
          ),
          hr: () => <hr className="border-border-divider" />,
          img: ({ alt }): ReactNode =>
            alt ? <span className="text-fg-subtle">[{alt}]</span> : null,
        }}
      >
        {markdown}
      </Markdown>
    </div>
  );
}
