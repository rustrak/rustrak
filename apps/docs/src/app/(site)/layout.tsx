import '@fontsource-variable/inter/opsz.css';
import '@fontsource-variable/geist-mono';
import { Head } from 'nextra/components';
import './globals.css';

/*
 * Inter and Geist Mono, self-hosted the same way as the landing and the
 * dashboard, so every Rustrak surface resolves the same files.
 */

export const metadata = {
  title: {
    default: 'Rustrak Documentation',
    template: '%s - Rustrak',
  },
  description: 'Self-hosted error tracking compatible with Sentry SDKs',
};

/**
 * The document, and nothing else.
 *
 * This used to also be the documentation shell — `getPageMap()`, the theme's
 * `<Layout>`, the navbar and the footer all lived here, which meant every route
 * got them whether it wanted them or not. They have moved to `(docs)/layout`,
 * so the landing no longer carries a documentation theme it does not render.
 *
 * Nextra's `<Head>` stays. It has to be a direct child of `<html>` because it
 * renders a literal `<head>`, and unlike the theme it is genuinely shared: a
 * `<style>` block defining the `--nextra-*` custom properties and the two
 * `theme-color` metas. Both halves of the site want all three.
 *
 * It is deliberately given no `faviconGlyph`. That prop sets the letter `R` as
 * *text* inside an SVG data-URI, which is the one thing the mark is never
 * allowed to be — and it was drawing over a `favicon.ico` that was still the
 * retired bolt tile. Both are gone. The tab icon is now `icon.png` and
 * `apple-icon.png` beside this file, which Next serves by convention: the same
 * two files the dashboard ships, so every Rustrak surface answers with one
 * image. See `apps/dashboard/public/`.
 */
export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en" suppressHydrationWarning>
      {/* Nextra's primary and background, set to the tokens' lime and canvas
          (`--rk-lime-500`, `--surface-canvas`). The site is dark only. */}
      <Head
        color={{ hue: 83, saturation: 90, lightness: 69 }}
        backgroundColor={{ dark: 'rgb(22,22,22)', light: 'rgb(22,22,22)' }}
      />
      <body>{children}</body>
    </html>
  );
}
