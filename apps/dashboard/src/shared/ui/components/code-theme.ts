import type { CSSProperties } from 'react';

/**
 * The Prism theme for every code block in the product, made of the design
 * system's `--code-*` tokens rather than literals. Because it reads CSS
 * variables, one object serves both themes: switching to light repaints the
 * code with the page, with no second style to pick.
 */
const fg = 'var(--code-fg)';

export const codeTheme: Record<string, CSSProperties> = {
  'code[class*="language-"]': { color: fg, background: 'none' },
  'pre[class*="language-"]': { color: fg, background: 'none' },
  comment: { color: 'var(--code-comment)', fontStyle: 'italic' },
  prolog: { color: 'var(--code-comment)' },
  doctype: { color: 'var(--code-comment)' },
  cdata: { color: 'var(--code-comment)' },
  punctuation: { color: 'var(--code-punctuation)' },
  operator: { color: 'var(--code-punctuation)' },
  keyword: { color: 'var(--code-keyword)' },
  atrule: { color: 'var(--code-keyword)' },
  important: { color: 'var(--code-keyword)' },
  selector: { color: 'var(--code-keyword)' },
  string: { color: 'var(--code-string)' },
  char: { color: 'var(--code-string)' },
  'attr-value': { color: 'var(--code-string)' },
  regex: { color: 'var(--code-string)' },
  'template-string': { color: 'var(--code-string)' },
  url: { color: 'var(--code-string)' },
  number: { color: 'var(--code-number)' },
  boolean: { color: 'var(--code-number)' },
  constant: { color: 'var(--code-number)' },
  symbol: { color: 'var(--code-number)' },
  function: { color: 'var(--code-function)' },
  'function-variable': { color: 'var(--code-function)' },
  method: { color: 'var(--code-function)' },
  'class-name': { color: 'var(--code-type)' },
  builtin: { color: 'var(--code-type)' },
  'attr-name': { color: 'var(--code-type)' },
  tag: { color: 'var(--code-keyword)' },
  property: { color: fg },
  variable: { color: fg },
  parameter: { color: fg },
};
