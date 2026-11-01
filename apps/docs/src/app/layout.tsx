import { Geist, Geist_Mono } from 'next/font/google';
import { Head } from 'nextra/components';
import './globals.css';

/**
 * Geist and Geist Mono, the two families the brand assigns to everything that is not
 * the logo. Outfit is not here on purpose: it is the wordmark's typeface, the wordmark
 * is placed as outlines (`icons/rustrak-wordmark.tsx`), and loading a face to draw a
 * shape that is already drawn would be paying for it twice.
 *
 * `next/font/google` does not fetch at runtime. Next downloads the files during the
 * build and serves them from our own origin, so there is no request to Google from a
 * visitor's browser and no third-party in the critical path — which is what
 * `rustrak-brand`, `brand/assets/fonts/README.md` is really guarding against when it
 * says the type is never loaded from a CDN.
 *
 * What it does mean is that this is a second copy of Geist, versioned by Google rather
 * than by the brand repo. If the two ever diverge the symptom is metrics moving by a
 * hair between the docs site and every other Rustrak surface, with nothing failing. If
 * that ever matters, the fix is `next/font/local` pointed at
 * `rustrak-brand/brand/assets/fonts/`, whose hashes the brandbook records.
 */
const geistSans = Geist({
  variable: '--font-geist-sans',
  subsets: ['latin'],
});

const geistMono = Geist_Mono({
  variable: '--font-geist-mono',
  subsets: ['latin'],
});

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
      <Head>
        {/* The index of the whole site for agents (llmstxt.org, v2). */}
        <link rel="describedby" href="/llms.txt" />
      </Head>
      <body className={`${geistSans.variable} ${geistMono.variable} font-sans`}>
        {children}
      </body>
    </html>
  );
}
