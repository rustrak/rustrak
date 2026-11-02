# @rustrak/ui

The design system behind the Rustrak dashboard: pure grey, Inter, lime for the
one thing on the screen that is the point, and colour otherwise reserved for
severity.

Nothing imports it yet. It is developed and reviewed in Storybook.

```bash
pnpm storybook   -w @rustrak/ui   # http://localhost:6008
pnpm test        -w @rustrak/ui   # unit + component, in real Chromium
pnpm check-types -w @rustrak/ui
pnpm build       -w @rustrak/ui   # dist/ with ESM and types
```

## What holds it up

| Piece | Choice | Why |
|---|---|---|
| Primitives | [Base UI](https://base-ui.com) 1.7 | Accessibility, focus management and positioning, with no opinion about how anything looks. It is also what shadcn/ui builds on now, so the patterns are the ones the ecosystem already knows. |
| Styles | Tailwind 4.3 with `@theme` | The tokens are native CSS. Changing theme is rewriting variables, not recompiling. |
| Variants | `tailwind-variants` 3 | Slots, compound variants and `tailwind-merge` built in. `cva` would mean wiring the merge by hand in every multi-part component. |
| Icons | Hugeicons (stroke-rounded, free set) behind an adapter | See below. |
| Tests | Storybook 10 + Vitest 4 in real Chromium | Every story is a component test and every one goes through axe. |
| Bundling | `tsdown` | ESM, types, `"use client"` in the banner, and every dependency external. |

The shadcn CLI was not used. Its idea was taken — the code lives in the
repository and can be read and changed — but the components are written against
these tokens, not against its.

## The three rules

**1 · No loose values.** A component writes `bg-surface`, `h-control-md` or
`text-control`. Never `bg-[#1a1a1a]`, `h-[32px]` or `text-[12.5px]`.

Tailwind's factory namespaces are **reset** (`--color-*: initial`, and the same
for `--text-*`, `--radius-*`, `--shadow-*`, `--font-*`). In this package
`bg-red-500` and `text-sm` do not exist: they compile to nothing. If a utility
is not in `styles/tokens.css`, it does not belong to the design.

**2 · Three token layers.**

```
--rk-ink-800: #1a1a1a       primitive · a raw value, used by nobody
  └─ --surface              semantic  · says what it is for; changes with theme
       └─ --color-surface   Tailwind  · publishes the semantic as a utility
```

The light theme rewrites **only** the semantic layer: one block, not fifty
components. `lib/tokens.ts` mirrors the published names so `tailwind-merge`
knows `text-control` is a size and `text-fg-muted` a colour;
`lib/tokens.test.ts` compares the two files and fails if they drift.

**3 · State is read from the DOM.** Base UI hands out `data-active`,
`data-highlighted`, `data-disabled`, `data-popup-open`. The recipes read them as
variants (`data-active:font-semibold`) and containers read their children with
`has-*`. There are no state props to thread and no two sources of truth to
disagree.

## Where the palette comes from

Nothing here was invented. The greys and the lime are the product's own, and
the severity and chart ramps are its `sev-*` and `chart-*` verbatim, in oklch.

**Dark is the reference theme**, including with no `data-theme` attribute at
all. Rustrak is a dark product and every one of its screens is dark; light is
the opt-out (`data-theme="light"`) and is derived from the same source.

Two things in the palette are deliberately *not* a copy:

- **Severity has two values per level**, `--sev-error` and `--sev-error-fg`. A
  fill and a word need different lightness to read at the same strength on the
  same surface. The product uses both; this is where the pair gets named.
- **The light theme's lime is darker than the dashboard's.** `globals.css`
  paired `oklch(0.65 0.18 127)` with white text, which is **2.05:1** — the sort
  of thing that survives for years because nobody looks at a button and thinks
  to measure it. Same hue, stepped down until white clears AA at **5.74:1**.

`styles/contrast.test.ts` resolves the whole `var()` chain, converts oklch and
hex to sRGB and **pins** every ratio. Pinned rather than bounded on purpose: a
`toBeGreaterThan` lets a palette drift downwards for years as long as it stays
over the line, whereas a pinned number turns any colour change into a number
moving in the diff that somebody has to explain.

### What is below AA, and why

Under `--fg-subtle` the scale stops being body text:

| Token | On a panel | What it is for |
|---|---|---|
| `--fg-meta` | 4.04 | a timestamp beside an id, a count |
| `--fg-ghost` | 3.15 | a chevron, a separator, a path |
| `--fg-placeholder` | 2.43 | an empty field, a disabled hint |

None of them ever carries meaning alone: each repeats or annotates something set
in a tone that does clear AA. The ratios are pinned so a palette change shows up
in the diff instead of nowhere.

## Two type families, and the rule between them

Inter for prose, Geist Mono for machine text. The second family is not a
flourish: half of what this product shows was written by a machine and is read
as an identifier — an event id, a release tag, a stack frame, a duration, a
count that has to line up down a column.

> **If the value came out of the system and somebody might copy it out of the
> page, it is mono.**

One exception, and the design is explicit about it: a KPI is Inter at 24/600
with tabular figures. At that size mono stops reading as precision and starts
reading as a terminal.

## Motion

The durations and curves are **IBM Carbon**'s `productive` scale, the one
designed for dense work software. Material's was rejected: it is calibrated for
consumer and mobile, and its 225–375 ms feel slow when you press two hundred
controls a day.

| Utility | Value | For |
|---|---|---|
| `duration-instant` | 70 ms | colour, hover, focus |
| `duration-fast` | 110 ms | a press, a popup |
| `duration-moderate` | 150 ms | an indicator that slides |
| `duration-slow` | 240 ms | a panel, collapsing the sidebar |
| `ease-standard` | `0.2,0,0.38,0.9` | changes without moving |
| `ease-entrance` / `ease-exit` | — | arrives braking / leaves accelerating |

The ready-made combinations are in `lib/motion.ts`: `interactiveTransition`,
`pressScale`, `pressScaleTrigger`, `chevronFlip`, `popTransition`,
`slideTransition`, `swapAnimation`. Milliseconds and `cubic-bezier` are never
written by hand, and `transition-all` is never used.

What you notice: the button sinks 3 % when pressed; a button that opens a menu
stays sunk for as long as the menu is up, and its chevron turns over; the panel
grows out of the button it was opened from, rising 4 px as it arrives and
sinking back as it goes; the tab rule slides instead of jumping.
`prefers-reduced-motion` switches transitions and animations off globally.

Popups arrive over 150 ms and leave over 110 ms. Something you asked to see
deserves the time to be seen arriving; something you have dismissed should get
out of the way.

### The one thing to know before writing a transition

**Tailwind 4 never writes `transform`.** `scale-97`, `translate-x-*` and
`rotate-180` compile to the *individual* `scale`, `translate` and `rotate`
properties. So `transition-[transform]` transitions a property that never
changes: the value still jumps to its new state, it just jumps instantly, with
no easing and no duration.

Nothing throws and nothing looks wrong in a screenshot — it only feels dead.
This package shipped its first pass that way: every press, every popup and the
tab indicator were all snapping, and only opacity was ever animating. Every list
in `lib/motion.ts` now names the individual properties, and
`lib/motion.test.ts` fails if `transform` reappears inside a `transition-[…]`
anywhere in `src`, or if something scales or translates without a transition
that covers it.

The second half of the same lesson: **transitions do not inherit**. A chevron
inside a button does not pick up the button's transition, which is why
`chevronFlip` carries its own.

## The cursor

Tailwind 4 dropped `cursor: pointer` from buttons to match the browser. On a
desktop that makes sense — a native button already looks pressable. In a flat,
dense web application it does not: the hand is what separates "this does
something" from "this is a label".

`styles/base.css` puts it back, by role and not by tag, because Base UI draws
the checkbox, radio and switch as a `<span>`. Disabled gets `not-allowed`. Two
stories pin it.

## Icons go behind an adapter

Today it is Hugeicons (it replaced lucide, and that swap touched no
component). Three layers:

```
components/icon/icon.tsx                      the contract: IconComponent, sizes, stroke
components/icon/adapters/hugeicons.tsx        the only file that renders @hugeicons/react
components/icon/adapters/hugeicons-symbols.ts the only file that names upstream glyphs
components/icon/icon-catalog.ts               Rustrak names: IssuesIcon, MuteIcon, …
```

Components import from the catalogue, never from the library. The name says what
the icon **means** (`issues`, `mute`, `overflow`), not what it draws, so changing
the glyph does not force anyone to change anything. Size and stroke are imposed
by CSS, not passed as props: a CSS rule beats the `width`, `height` and
`stroke-width` attributes any library paints into the SVG.

## What is here

**Foundations** · `cn`, `tv`, `focusRing`, the tokens, the icon catalogue, the
wordmark.

**Presentation** · `Text` `Tag` `Count` `Kbd` `Avatar` `Separator` `Spinner`.

**Actions** · `Button` (primary · secondary · ghost · danger · danger-primary,
with an icon, a shortcut and a menu chevron) and `SplitButton`.

**Navigation** · `Tabs` `SegmentedControl` `Menu` `Tooltip` `Breadcrumbs`.

**Shell** · `AppShell` `Page` `PageHeader` `SubHeader` `Topbar` (the mark,
global search, notifications, account) `Sidebar` (216 px / a 56 px rail, the
project card, the seven routes, ⌘B).

**Forms** · `Checkbox` `Popover`.

**Forms, continued** · `Field` (`FieldLabel`/`FieldHint`/`FieldError`, wired
by Base UI), `Input` (leading symbol, trailing `InputAction`, numeric,
read-only, invalid), `Textarea` — all sharing one `inputShell` box.

**Charts** · recharts 3 behind three components that never leak it:
`TimeSeriesChart` (areas with gradient fills, stacked or overlaid),
`BarsChart` (bucketed, stacked by default, 1 px surface seams between
bands), and `Sparkline` (hand-drawn SVG for table rows: no store, no
animation, no tooltip). Colours are `var(--chart-*)` and the severity
tokens; both palettes pass the CVD/contrast validation. TanStack Charts was
evaluated and parked: relaunched 2026-07, pre-alpha, to be revisited at 1.0.

**Waterfall** · `Waterfall`, the trace view: split columns with a draggable
divider, durations riding their bars, chevron pills carrying descendant
counts, tree guide lines, autogrouping of identical leaf siblings (the N+1),
optional hatched missing-instrumentation rows, and a roving-tabindex tree
for the keyboard. Colour by kind — db / internal / http — plus the error as
the page's only red.

**Overlays** · `ToastProvider`/`useToast` (a stacked corner notice: tones
with their own lifetimes, actions, progress, `promise`), `Dialog` with its
`Header`/`Body`/`Footer`, `createDialog`/`DialogProvider` (modals opened by
calling and awaited), and `confirm`/`alert` on top.

**Data** · the table family — `useDataTable` (TanStack Table v9, fully manual)
with `DataTable`, `DataTablePagination`, `DataTableColumnsButton`, the
column-header sort/filter panels behind them — and `QueryBar`, the token
search with two-phase autocomplete. `data-table/query.ts` holds the shared
query model and the URL codecs.

### Named decisions

- **`Tag` defaults to plain coloured text, not a filled pill.** An issue list is
  already a grid of rows and rules, and a pill on every row turned severity into
  confetti. `variant="soft"` brings the fill back for a label standing on its
  own, with nothing nearby to read it against.
- **The sidebar holds the project and the routes, and nothing else.** Earlier
  passes carried an environment selector and a quota block; both went. A
  navigation column that also holds controls stops being scannable, and anything
  that is a control belongs on the page it controls.
- **The left of the topbar holds the mark and nothing else.** The organisation
  and its switcher used to sit beside it and went for the same reason: the
  moment a frame holds a control it stops being a frame. Which project you are
  in is said by the sidebar, right above the routes it governs; which
  organisation, by the account menu on the right.
- **`SegmentedControl` is a radio group, not a toggle group.** It used to be
  built on toggles, and clicking the chosen option switched it off: a control
  reading "1 h · 24 h · 7 d" with none of them lit, and a chart with no range
  behind it. There is no such state. A set of choices where exactly one is
  always taken *is* a radio group, so it is built on one — which means the
  browser refuses to empty it, arrow keys move through the options, and a
  screen reader announces "1 of 6" instead of six unrelated toggle buttons.
  Nothing was written to prevent the empty state; the right primitive does not
  have it. Its chip slides from the old option to the new one for the same
  reason the tab rule does: the travel is what says only the choice moved.
- **The table is fully manual.** `useDataTable` registers no client row
  models: the server filters, sorts and paginates, and the table renders what
  it was handed. State goes out as pure updaters on a `DataTableQuery`, and
  the package never touches a router — `parseTableQuery` and
  `serializeTableQuery` are the codecs; the app owns the URL. Any change of
  filters, search or sort rewinds `pageIndex` to 0 in the same update,
  because manual pagination switches TanStack's own auto-reset off.
- **A column header opens a panel; it does not sort on click.** The GitHub
  pattern: sorting, the type-appropriate filter (options, text, range — from
  `meta.filter`) and hiding share one panel, with the sort worded in the
  column's own terms ("Most events first"). The resting header shows state
  only: a lime arrow when sorted, a lime funnel when filtered.
- **The query bar and the column panels edit the same `ColumnFiltersState`.**
  A tick in the Level panel is a `level:` chip in the bar; deleting the chip
  puts the funnel out. The bar's text form is exactly what
  `parseFilterQuery` reads, so the URL's `q`, the bar's content and the
  table's filters are one value in three places.
- **Row actions rest in a fixed ⋯ column; bulk actions take over the
  header.** Hover-revealed actions went: what only exists under the pointer
  cannot be discovered by reading and does not exist on touch. Every row ends
  in the same ⋯ menu (`rowMenu`, as `MenuAction[]`), and once rows are
  selected the 38 px header strip swaps -- with `swapAnimation`, both
  directions -- into count, `bulkActions` and Clear, so nothing pushes the
  table down mid-gesture.
- **Modals open by being called.** `createDialog` returns `open()`, which
  resolves with the answer; a `DialogProvider` at the root mounts them inside
  the tree, so context still holds. `confirm()` returns `true` only when the
  action is pressed -- Escape, like Cancel, is a no -- and the irreversible
  asks for its phrase to be typed.
- **A toast's lifetime follows from its tone.** Confirmations leave in 5 s
  with a countdown at the foot, undo waits 8 s, and what demands action
  (`warning`, `danger`) stays until dismissed. The stack peeks and opens on
  hover; the geometry rides the individual `translate`/`scale` properties,
  like all motion here.
- **Selection and column visibility never reach the URL.** A selection is a
  gesture and visibility is a reading preference; restoring either from a
  pasted link would fabricate a choice the reader never made. They live
  inside `useDataTable`.

## Consuming it from an application

```tsx
import { AppShell, Button, Sidebar } from '@rustrak/ui';
```

In `globals.css`:

```css
@import 'tailwindcss';
@import '@rustrak/ui/styles.css';
@source '../node_modules/@rustrak/ui/dist';
```

The `@source` is **not optional**: Tailwind only emits the utilities it sees, and
this package's classes live in its own bundle.

The fonts are the application's job. Inter and Geist Mono are on Google Fonts;
Storybook loads them from there in `.storybook/preview-head.html` for the
stories and `.storybook/manager-head.html` for its own interface.

## Storybook wears the system

`.storybook/theme.ts` builds a theme from the same palette the components use,
and `.storybook/manager.ts` applies it: the lime wordmark in the sidebar header,
the four surfaces in the right stacking order, Inter and Geist Mono throughout,
lime for selection and links. The docs pages get the same theme through
`parameters.docs.theme`.

Two things are worth knowing about it.

**The values are duplicated, and they have to be.** The manager is a separate
bundle from the preview: it never loads the stories' stylesheet, so it cannot
read a CSS custom property. Every line in `theme.ts` names the token it is
copying, so a palette change has one obvious second place to go.

**The mark is lime here and white in the product.** Deliberate: this is the
workshop, not the application, and the lime mark makes the tab findable in a row
of twenty. `.storybook/public/rustrak-wordmark.svg` is generated from the same
paths as `components/brand/wordmark.tsx`.

`.storybook/manager-head.html` carries the handful of things `create()` cannot
reach — the mark's size, the small-caps section headings. It is the only part of
the setup written against Storybook's own class names, so it is the only part
that can break on an upgrade; it is all cosmetic, and a break degrades to the
stock look rather than to a broken page.

The story background follows the theme switch because the `backgrounds` addon is
handed `var(--surface-canvas)` rather than a hex. Given a literal, switching the
theme changed the components while the page around them stayed black, which is
exactly the sort of thing that makes a light theme look broken when it is not.

## Accessibility

Every story runs in a real Chromium and goes through axe. What the system
guarantees:

- a visible focus ring on everything focusable, the same 2 px lime everywhere;
- an icon-only button **requires** `aria-label`, and the type checks it at
  compile time: `<Button icon={X} />` with no name does not get past `tsc`;
- state never rests on colour alone — the selected tab and the active route
  gain weight as well as a background;
- a scrolling `Page` is focusable, because a scroll box with nothing focusable
  inside cannot be paged with a keyboard at all;
- `prefers-reduced-motion` switches transitions and animations off.

Two axe rules are switched off in `.storybook/preview.tsx`: `color-contrast`,
which is covered properly in `contrast.test.ts` against the rule that actually
applies to this palette, and `aria-hidden-focus`, which flags the focus guards
Base UI injects into its own portals.

## Repository notes

`biome.json` switches off `a11y/useAnchorContent` and `a11y/useHeadingContent`
for this package only. Both rules stop at the JSX tag, and here the tag is chosen
with Base UI's `render` prop — `<Text render={<h2 />}>` — so the linter sees an
empty element while the content arrives from the component. What actually
guarantees it is the axe run over the rendered DOM, which is stricter than
either rule.
