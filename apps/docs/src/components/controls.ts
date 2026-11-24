/*
 * The few controls on the landing that do something, without React.
 *
 * They are `@rustrak/ui` components rendered to HTML at build time, so the
 * markup and the data attributes their styles read are already Base UI's;
 * this only moves those attributes when they are used.
 */

/** Tabs: `[data-tabs]` holds the tabs and, in the same order, the panels. */
function tabs(root: HTMLElement) {
  const list = root.querySelector<HTMLElement>('[role="tablist"]');
  const tabs = [...root.querySelectorAll<HTMLElement>('[role="tab"]')];
  const panels = [...root.querySelectorAll<HTMLElement>('[role="tabpanel"]')];
  const indicator = list?.querySelector<HTMLElement>('[role="presentation"]');

  tabs.forEach((tab, i) => {
    tab.setAttribute('aria-controls', panels[i].id);
    panels[i].setAttribute('aria-labelledby', tab.id);
  });

  const select = (index: number) => {
    tabs.forEach((tab, i) => {
      const on = i === index;
      tab.toggleAttribute('data-active', on);
      tab.setAttribute('aria-selected', String(on));
      tab.tabIndex = on ? 0 : -1;
      panels[i].hidden = !on;
      panels[i].inert = !on;
      panels[i].toggleAttribute('data-hidden', !on);
    });
    const tab = tabs[index];
    indicator?.style.setProperty('--active-tab-left', `${tab.offsetLeft}px`);
    indicator?.style.setProperty('--active-tab-width', `${tab.offsetWidth}px`);
  };

  const current = () => tabs.findIndex((t) => t.hasAttribute('data-active'));
  tabs[current()].tabIndex = 0;
  tabs.forEach((tab, i) => {
    tab.addEventListener('click', () => select(i));
  });
  list?.addEventListener('keydown', (e) => {
    const step = { ArrowRight: 1, ArrowLeft: -1 }[e.key];
    if (!step) return;
    const next = (current() + step + tabs.length) % tabs.length;
    select(next);
    tabs[next].focus();
  });
}

/**
 * A segmented control that shows one of several blocks: `[data-switch]` holds
 * the radio group and the `[data-option]` blocks named by its values.
 */
function segmented(root: HTMLElement) {
  const radios = [...root.querySelectorAll<HTMLElement>('[role="radio"]')];
  const values = radios.map(
    (radio) => (radio.nextElementSibling as HTMLInputElement | null)?.value,
  );
  const select = (index: number) => {
    radios.forEach((radio, i) => {
      const on = i === index;
      radio.toggleAttribute('data-checked', on);
      radio.toggleAttribute('data-unchecked', !on);
      radio.setAttribute('aria-checked', String(on));
      radio.tabIndex = on ? 0 : -1;
    });
    for (const block of root.querySelectorAll<HTMLElement>('[data-option]')) {
      block.hidden = block.dataset.option !== values[index];
    }
  };
  radios.forEach((radio, i) => {
    radio.addEventListener('click', () => select(i));
    radio.addEventListener('keydown', (e) => {
      const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[
        e.key
      ];
      if (e.key === ' ' || e.key === 'Enter') select(i);
      if (!step) return;
      e.preventDefault();
      const next = (i + step + radios.length) % radios.length;
      select(next);
      radios[next].focus();
    });
  });
  // Rendered outside the browser, the group's roving tabindex leaves every
  // radio at -1, and the control out of the Tab order. The checked one takes
  // it back.
  select(
    Math.max(
      0,
      radios.findIndex((r) => r.hasAttribute('data-checked')),
    ),
  );
}

/**
 * Copies the visible `[data-option]` block's `code`. The copied state is
 * the second button in `[data-copy]`, shown for a moment.
 */
function copy(root: HTMLElement) {
  const holder = root.querySelector<HTMLElement>('[data-copy]');
  const [idle, done] = holder?.querySelectorAll('button') ?? [];
  if (!idle || !done) return;
  let timer = 0;
  for (const button of [idle, done]) {
    button.addEventListener('click', () => {
      const code = root.querySelector('[data-option]:not([hidden]) code');
      // `clipboard` is missing outside secure contexts.
      navigator.clipboard?.writeText(code?.textContent ?? '').then(
        () => {
          idle.hidden = true;
          done.hidden = false;
          clearTimeout(timer);
          timer = window.setTimeout(() => {
            idle.hidden = false;
            done.hidden = true;
          }, 1600);
        },
        () => {},
      );
    });
  }
}

for (const root of document.querySelectorAll<HTMLElement>('[data-tabs]')) {
  tabs(root);
}
for (const root of document.querySelectorAll<HTMLElement>('[data-switch]')) {
  segmented(root);
  copy(root);
}
