/*
 * A rehype plugin for the docs' code blocks, so they arrive in the HTML
 * finished rather than rebuilt by a script after the first paint.
 *
 * - Each block gets a copy button and, if it says, its language.
 * - Blocks that follow one another with nothing between them, in different
 *   languages, are the same example in several SDKs: they become one block
 *   with a tab per language, as Resend and Vercel draw them. The Markdown
 *   needs nothing for it.
 *
 * `components/docs/code-blocks.ts` wires the buttons and the tabs.
 */

const NAMES = {
  javascript: 'JavaScript',
  js: 'JavaScript',
  typescript: 'TypeScript',
  ts: 'TypeScript',
  python: 'Python',
  go: 'Go',
  rust: 'Rust',
  ruby: 'Ruby',
  php: 'PHP',
  java: 'Java',
  kotlin: 'Kotlin',
  swift: 'Swift',
  csharp: 'C#',
  bash: 'Shell',
  shell: 'Shell',
  yaml: 'YAML',
  json: 'JSON',
  toml: 'TOML',
};

/** Only the documentation's pages; the blog and the changelog keep plain blocks. */
const DOCS = /\/content\/(?!blog\/|changelog\/)/;

const el = (tagName, properties, children = []) => ({
  type: 'element',
  tagName,
  properties,
  children,
});
const text = (value) => ({ type: 'text', value });
const isPre = (node) => node?.type === 'element' && node.tagName === 'pre';
const langOf = (pre) => pre.properties?.dataLanguage ?? '';
const blank = (node) => node?.type === 'text' && !node.value.trim();
const copy = () =>
  el('button', { type: 'button', className: ['code-copy'] }, [text('Copy')]);

function single(pre) {
  const lang = langOf(pre);
  const tag =
    lang && lang !== 'plaintext' && lang !== 'text'
      ? [el('span', { className: ['code-lang'] }, [text(NAMES[lang] ?? lang)])]
      : [];
  return el('div', { className: ['code-block'] }, [pre, ...tag, copy()]);
}

function group(run) {
  const tabs = run.map((pre, i) =>
    el(
      'button',
      {
        type: 'button',
        role: 'tab',
        dataLang: langOf(pre),
        ariaSelected: String(i === 0),
        tabIndex: i === 0 ? 0 : -1,
      },
      [text(NAMES[langOf(pre)])],
    ),
  );
  for (const pre of run.slice(1)) pre.properties.hidden = true;
  return el('div', { className: ['code-block', 'code-group'] }, [
    el('div', { className: ['code-tabs'], role: 'tablist' }, tabs),
    ...run,
    copy(),
  ]);
}

/** Groups and wraps the `<pre>`s among `node`'s children, at every depth. */
function wrap(node) {
  if (!node.children) return;
  const out = [];
  const kids = node.children;
  for (let i = 0; i < kids.length; i++) {
    const child = kids[i];
    if (!isPre(child)) {
      wrap(child);
      out.push(child);
      continue;
    }
    const run = [child];
    for (;;) {
      let next = i + 1;
      while (blank(kids[next])) next++;
      const pre = kids[next];
      const joins =
        isPre(pre) &&
        NAMES[langOf(run[0])] &&
        NAMES[langOf(pre)] &&
        !run.some((p) => langOf(p) === langOf(pre));
      if (!joins) break;
      run.push(pre);
      i = next;
    }
    out.push(run.length === 1 ? single(child) : group(run));
  }
  node.children = out;
}

export const codeBlocks = () => (tree, file) => {
  if (DOCS.test(file.path ?? '')) wrap(tree);
};
