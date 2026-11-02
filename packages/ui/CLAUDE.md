# @rustrak/ui

The design system. Nothing imports it yet: it is developed and reviewed in
Storybook. Root context: `/CLAUDE.md`. The full
rationale — palette provenance, motion scale, the icon adapter, what is below AA
and why — is in [`README.md`](README.md). **Read that before changing a token.**

```bash
pnpm storybook   -w @rustrak/ui   # http://localhost:6008
pnpm test        -w @rustrak/ui   # unit + component, in real Chromium
pnpm check-types -w @rustrak/ui
```

## Layout

```
src/
├── styles/      tokens.css, base.css, and the contrast test that pins the palette
├── lib/         cn, tv, tw-merge, tokens.ts, motion, focus, types
├── docs/        the blocks the documentation pages are written with
└── components/
    └── <name>/  <name>.tsx, <name>.stories.tsx, <name>.mdx
```

One folder per component, flat, and each holds three files: the component, its
stories and its documentation page. There is no `primitives/` versus
`composites/` split: it would put `Button` and `Sidebar` in different places for a reason
nobody consuming the package can see.

## The rules, short

1. **No loose values.** `bg-surface`, `h-control-md`, `text-control`. Never a
   hex, a pixel or a `text-[13px]`. Tailwind's factory namespaces are reset, so
   `bg-red-500` compiles to nothing — if it is not in `styles/tokens.css` it is
   not in the design.
2. **Three token layers**: `--rk-*` primitive → `--surface` semantic →
   `--color-surface` published. Only the semantic layer changes with the theme.
   Dark is the default, including with no `data-theme` at all.
3. **State comes from the DOM.** Read Base UI's `data-active`,
   `data-highlighted`, `data-disabled`, `data-popup-open` as variants. Never add
   a state prop that duplicates one.
4. **Every recipe is a `tv`.** Never a template string of classes, never
   `clsx` in a component body — `cn` is only for merging an incoming
   `className`.
5. **Motion comes from `lib/motion.ts`.** Never a hand-written duration or
   `cubic-bezier`, never `transition-all`, and **never `transition-[transform]`**
   — Tailwind 4 compiles `scale-*`, `translate-*` and `rotate-*` to the
   individual properties, so naming `transform` transitions nothing at all and
   the change lands in one frame. `lib/motion.test.ts` enforces it. Transitions
   do not inherit either: a child that moves needs its own.

## Adding a component

Write the recipe, the component, the stories and the page in one change — the
stories *are* the tests. A story runs in Chromium and goes through axe, so a
component with no story has no test at all, and one with no `.mdx` has no
documentation at all: autodocs is off on purpose.

What the stories have to cover, because these are the things that rot:

- every variant and every size in one **States** story, side by side. It is the
  comparison that reveals drift; a page of separate stories does not.
- the keyboard path, with `play`. Tab to it, act on it, assert.
- whatever the component promises that is not visual: `aria-pressed` on a
  toggled button, `aria-current` on the active route, focus returning to the
  trigger when a popup closes.

Three things that will bite:

- **Popups start at `opacity: 0`** — that is `popTransition` doing its job. Use
  `findByRole` and `toBeInTheDocument`, never `toBeVisible`, or the assertion
  races the entry transition.
- **Focus comes back after the exit transition**, not immediately. Wrap the
  assertion in `waitFor`.
- **Do not assert computed style on a portalled popup.** The stylesheet does not
  resolve against nodes outside the story root in the Vitest browser
  environment, so `transitionProperty` comes back `none` for a popup whose
  classes are demonstrably right. Assert the class list there; computed style is
  reliable for anything inside the root, and `lib/motion.test.ts` covers the
  rest.

Base UI swallows `:active` on anything that opens a popup — it opens on
pointer-down and hands capture to the panel. Use `pressScaleTrigger`, which
reads `data-pressed` instead, or the control gives no feedback at all until the
panel appears.

## Adding a documentation page

Every component has a written `<name>.mdx` beside its stories, attached with
`<Meta of={Stories} />`. That page **replaces** the generated one, which is why
autodocs is off: a page exists because somebody wrote it.

The shape is fixed, and it is the brandbook's — cover, numbered sections
divided by a hairline, one `Rule` per section:

1. **Cover** — eyebrow, name, one line saying what it is.
2. **Anatomy** — the live component with its parts numbered.
3. **Variants**, and then **every variant against every state** as a `Matrix`.
4. **Motion**, where the component has any worth replaying.
5. **Guidelines** — `Do`/`Dont` pairs, both halves, rendered live.
6. **Where it is used** — the actual screens. This is the part no props table
   answers and the part that decides how the component gets used.
7. **What it guarantees** — one line per promise, each naming the story that
   asserts it.
8. **Props** — `<Controls />`, generated from the types.

The blocks are in `src/docs/blocks`. Three rules about them:

- the whole page goes inside `<Unstyled>`. Storybook's docs stylesheet has its
  own container width, margins and headings, and the spacing has to come from
  one place or it comes from two.
- **MDX is not type-checked.** A wrong prop compiles and fails at runtime, so a
  new page is opened in the browser before it is called done.
- **Nothing else reads MDX either.** `react-doctor` parses `.tsx` only, so every
  block in `src/docs/blocks` looks like a file nobody imports. That is what
  `doctor.config.json` in this package is for, and it is scoped here rather than
  at the root so the pattern cannot match some other package's `src/docs`. It
  also silences `heading-has-content` on `page.tsx`: the rule reads the element
  given to Base UI's `render` prop literally and sees `<h2 />` with no children.

`src/docs/overview.mdx` is the front page and `src/styles/*.mdx` are the
Foundations. There are no token *stories*: those pages are the specimens now.

## Adding a token

`styles/tokens.css` and `lib/tokens.ts` are two halves of one thing, and
`lib/tokens.test.ts` fails the moment they disagree — tailwind-merge cannot read
CSS, so without the mirror a `className` passed in from outside silently stops
overriding the component's own.

A colour also needs a line in `styles/contrast.test.ts`. The ratios there are
**pinned, not bounded**: a change to any colour has to show up in the diff as a
number moving. Do not relax an assertion to make it pass — either the change is
right and the new number gets written down, or it is not.

## Do not

- add a `dark:` variant to a component. The semantic layer already handles the
  theme; a `dark:` in a recipe means a token is missing.
- import `@hugeicons/*` anywhere except `components/icon/adapters/`.
- build classes with template literals. Tailwind extracts statically, so
  `bg-${name}` is a rule that is silently never generated.
- start a parallel table, filter bar or chart. They exist: `data-table/`,
  `query-bar/` and `chart/` (recharts behind `TimeSeriesChart`/`BarsChart`,
  hand-drawn `Sparkline` for rows) — extend those. Chart colours are
  `var(--chart-*)` and the severity tokens, validated as a palette; never
  hand a chart a new colour without re-running that validation.
- give the `backgrounds` addon a hex. It writes its value onto the preview body,
  so a literal pins the page to one theme while the components inside it follow
  the other. Hand it `var(--surface-canvas)`.
- change a colour in `styles/tokens.css` without checking `.storybook/theme.ts`.
  The manager is a separate bundle and cannot read a CSS variable, so that file
  is a hand-kept copy. Every line in it names the token it copies.
