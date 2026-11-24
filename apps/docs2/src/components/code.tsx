import type { ReactNode } from 'react';

const TOKEN =
  /(\/\/.*|#.*|"[^"]*"|'[^']*'|`[^`]*`|\b(?:import|from|const|let|await|async|function|return|throw|new|def|raise|package|func|use|fn|let|require|if|else|do|end)\b|\b[a-zA-Z_]\w*(?=\())/g;

/**
 * Enough highlighting for a dozen lines of example code: comments, strings,
 * keywords and calls, in the product's own code colours. Not a parser.
 */
export function highlight(line: string): ReactNode[] {
  const out: ReactNode[] = [];
  let last = 0;
  for (const match of line.matchAll(TOKEN)) {
    const [token] = match;
    const at = match.index ?? 0;
    if (at > last) out.push(line.slice(last, at));
    last = at + token.length;
    out.push(
      <span key={at} className={toneOf(token, line[last] === '(')}>
        {token}
      </span>,
    );
  }
  if (last < line.length) out.push(line.slice(last));
  return out;
}

function toneOf(token: string, called: boolean): string {
  if (token.startsWith('//') || token.startsWith('#'))
    return 'text-code-comment';
  if (/^["'`]/.test(token)) return 'text-code-string';
  if (/^[a-z]+$/.test(token) && !called) return 'text-code-keyword';
  return 'text-code-function';
}
