/*
 * Wires the docs' code blocks, which `lib/code-blocks.mjs` lays out at build
 * time: the copy buttons, and the language tabs. The language picked is
 * remembered, so every group on every page opens on it.
 */

const STORE = 'docs-language';

for (const button of document.querySelectorAll<HTMLButtonElement>(
  '.code-copy',
)) {
  button.addEventListener('click', () => {
    const pre = [...(button.parentElement?.querySelectorAll('pre') ?? [])].find(
      (p) => !p.hidden,
    );
    navigator.clipboard
      ?.writeText(pre?.querySelector('code')?.textContent ?? '')
      .then(
        () => {
          button.textContent = 'Copied';
          setTimeout(() => {
            button.textContent = 'Copy';
          }, 1600);
        },
        () => {},
      );
  });
}

/** Every group that has `lang` shows it. */
function show(lang: string) {
  for (const group of document.querySelectorAll('.code-group')) {
    const pres = [...group.querySelectorAll('pre')];
    const index = pres.findIndex((pre) => pre.dataset.language === lang);
    if (index < 0) continue;
    const tabs = group.querySelectorAll<HTMLElement>('[role=tab]');
    pres.forEach((pre, i) => {
      pre.hidden = i !== index;
      tabs[i].setAttribute('aria-selected', String(i === index));
    });
  }
}

document.addEventListener('click', (event) => {
  const tab = (event.target as Element).closest<HTMLElement>(
    '.code-tabs [role=tab]',
  );
  const lang = tab?.dataset.lang;
  if (!lang) return;
  try {
    localStorage.setItem(STORE, lang);
  } catch {}
  show(lang);
});

try {
  const saved = localStorage.getItem(STORE);
  if (saved) show(saved);
} catch {}
