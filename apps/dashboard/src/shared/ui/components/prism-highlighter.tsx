/// <reference types="react-syntax-highlighter" />

import type { CSSProperties } from 'react';
import bash from 'react-syntax-highlighter/dist/esm/languages/prism/bash';
import c from 'react-syntax-highlighter/dist/esm/languages/prism/c';
import cpp from 'react-syntax-highlighter/dist/esm/languages/prism/cpp';
import csharp from 'react-syntax-highlighter/dist/esm/languages/prism/csharp';
import css from 'react-syntax-highlighter/dist/esm/languages/prism/css';
import dart from 'react-syntax-highlighter/dist/esm/languages/prism/dart';
import elixir from 'react-syntax-highlighter/dist/esm/languages/prism/elixir';
import go from 'react-syntax-highlighter/dist/esm/languages/prism/go';
import java from 'react-syntax-highlighter/dist/esm/languages/prism/java';
import javascript from 'react-syntax-highlighter/dist/esm/languages/prism/javascript';
import json from 'react-syntax-highlighter/dist/esm/languages/prism/json';
import jsx from 'react-syntax-highlighter/dist/esm/languages/prism/jsx';
import kotlin from 'react-syntax-highlighter/dist/esm/languages/prism/kotlin';
import less from 'react-syntax-highlighter/dist/esm/languages/prism/less';
import markdown from 'react-syntax-highlighter/dist/esm/languages/prism/markdown';
import markup from 'react-syntax-highlighter/dist/esm/languages/prism/markup';
import php from 'react-syntax-highlighter/dist/esm/languages/prism/php';
import python from 'react-syntax-highlighter/dist/esm/languages/prism/python';
import ruby from 'react-syntax-highlighter/dist/esm/languages/prism/ruby';
import rust from 'react-syntax-highlighter/dist/esm/languages/prism/rust';
import scss from 'react-syntax-highlighter/dist/esm/languages/prism/scss';
import sql from 'react-syntax-highlighter/dist/esm/languages/prism/sql';
import swift from 'react-syntax-highlighter/dist/esm/languages/prism/swift';
import tsx from 'react-syntax-highlighter/dist/esm/languages/prism/tsx';
import typescript from 'react-syntax-highlighter/dist/esm/languages/prism/typescript';
import yaml from 'react-syntax-highlighter/dist/esm/languages/prism/yaml';
import SyntaxHighlighter from 'react-syntax-highlighter/dist/esm/prism-light';
import vs from 'react-syntax-highlighter/dist/esm/styles/prism/vs';
import vscDarkPlus from 'react-syntax-highlighter/dist/esm/styles/prism/vsc-dark-plus';

// The light build ships no grammar of its own. This list is every language
// the stack frames and the setup snippets can name; anything else renders
// as plain text.
const LANGUAGES = {
  bash,
  c,
  cpp,
  csharp,
  css,
  dart,
  elixir,
  go,
  html: markup,
  java,
  javascript,
  json,
  jsx,
  kotlin,
  less,
  markdown,
  php,
  python,
  ruby,
  rust,
  scss,
  sql,
  swift,
  tsx,
  typescript,
  xml: markup,
  yaml,
};

for (const [name, grammar] of Object.entries(LANGUAGES)) {
  SyntaxHighlighter.registerLanguage(name, grammar);
}

export interface PrismHighlighterProps {
  code: string;
  language: string;
  dark: boolean;
  customStyle?: CSSProperties;
  codeTagProps?: React.HTMLAttributes<HTMLElement>;
}

/** Loaded through `CodeHighlighter`, never imported directly. */
export default function PrismHighlighter({
  code,
  language,
  dark,
  customStyle,
  codeTagProps,
}: PrismHighlighterProps) {
  return (
    <SyntaxHighlighter
      language={language in LANGUAGES ? language : 'text'}
      style={dark ? vscDarkPlus : vs}
      customStyle={customStyle}
      codeTagProps={codeTagProps}
    >
      {code}
    </SyntaxHighlighter>
  );
}
