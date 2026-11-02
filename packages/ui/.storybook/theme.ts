import { create } from 'storybook/theming';

/**
 * Storybook wearing the design system it documents.
 *
 * The values are the same ones in `src/styles/tokens.css`, written out as
 * literals because Storybook's manager is a separate bundle: it does not load
 * the preview's stylesheet, so it cannot read a CSS custom property. That is a
 * genuine duplication and the only one in the package — if a colour moves in
 * the tokens, it has to move here too, which is why every line below says which
 * token it is copying.
 *
 * The workshop is branded in lime rather than in the product's white wordmark.
 * It is a deliberate difference: this is the tool, not the application, and the
 * lime mark makes the tab findable in a row of twenty. One line to change if it
 * ever reads as the product.
 */
export const rustrakTheme = create({
  base: 'dark',

  brandTitle: 'Rustrak UI',
  brandUrl: 'https://github.com/rustrak/rustrak',
  brandImage: './rustrak-wordmark.svg',
  brandTarget: '_blank',

  // --surface-brand / --fg-accent
  colorPrimary: '#c1f768',
  colorSecondary: '#c1f768',

  // The four surfaces, in the order they stack: canvas, panel, surface.
  appBg: '#0f0f0f', // --surface-canvas
  appContentBg: '#171717', // --surface-panel
  appPreviewBg: '#0f0f0f', // --surface-canvas
  appBorderColor: '#262626', // --border over the panel
  appBorderRadius: 8, // --radius-lg
  /*
   * The row you are pointing at in the sidebar.
   *
   * It is the last of the five colours Storybook's dark theme sets by hand, and
   * the only one the documented shorthands do not reach -- `create()` takes it,
   * it is just not in the list anybody reads. Left alone it is `#233952`, a
   * navy tint of Storybook's own blue, so every hover in a lime sidebar flashed
   * blue. This is `--surface-hover`, the same tint the product puts under a nav
   * row.
   */
  appHoverBg: '#1c1c1c', // --surface-hover over the canvas

  fontBase: '"Inter", ui-sans-serif, system-ui, sans-serif',
  fontCode: '"Geist Mono", ui-monospace, "SF Mono", Menlo, monospace',

  textColor: '#f1f1f1', // --fg
  textInverseColor: '#0f0f0f', // --fg-inverse
  textMutedColor: '#808080', // --fg-subtle

  // The toolbar over the story: the same surface a topbar uses.
  barBg: '#1b1b1b', // --surface
  barTextColor: '#808080', // --fg-subtle
  barHoverColor: '#d4d4d4', // --fg-secondary
  barSelectedColor: '#c1f768', // --fg-brand

  inputBg: '#121315', // --surface
  inputBorder: '#212224', // --border over the panel
  inputTextColor: '#f1f2f4', // --fg
  inputBorderRadius: 7, // --radius-md
});
