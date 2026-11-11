import { create } from 'storybook/theming';

declare global {
  interface Window {
    /** Resolved from `tokens.css` by `main.ts`, in the manager's head. */
    RUSTRAK_TOKENS: Record<
      | 'brand'
      | 'canvas'
      | 'panel'
      | 'surface'
      | 'hover'
      | 'border'
      | 'fg'
      | 'fgSecondary'
      | 'fgSubtle'
      | 'fgInverse',
      string
    > &
      Record<'radiusLg' | 'radiusMd', number>;
  }
}

const t = window.RUSTRAK_TOKENS;

/**
 * Storybook wearing the design system it documents, with every value read
 * from `src/styles/tokens.css` (see `main.ts`).
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

  colorPrimary: t.brand,
  colorSecondary: t.brand,

  // The surfaces, in the order they stack: canvas, panel, surface.
  appBg: t.canvas,
  appContentBg: t.panel,
  appPreviewBg: t.canvas,
  appBorderColor: t.border,
  appBorderRadius: t.radiusLg,
  /*
   * The row you are pointing at in the sidebar.
   *
   * It is the last of the five colours Storybook's dark theme sets by hand, and
   * the only one the documented shorthands do not reach -- `create()` takes it,
   * it is just not in the list anybody reads. Left alone it is `#233952`, a
   * navy tint of Storybook's own blue, so every hover in a lime sidebar flashed
   * blue.
   */
  appHoverBg: t.hover,

  fontBase: '"Inter", ui-sans-serif, system-ui, sans-serif',
  fontCode: '"Geist Mono", ui-monospace, "SF Mono", Menlo, monospace',

  textColor: t.fg,
  textInverseColor: t.fgInverse,
  textMutedColor: t.fgSubtle,

  // The toolbar over the story: the same surface a topbar uses.
  barBg: t.surface,
  barTextColor: t.fgSubtle,
  barHoverColor: t.fgSecondary,
  barSelectedColor: t.brand,

  inputBg: t.surface,
  inputBorder: t.border,
  inputTextColor: t.fg,
  inputBorderRadius: t.radiusMd,
});
