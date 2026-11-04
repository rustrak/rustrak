import '@fontsource-variable/inter/opsz.css';
import '@fontsource-variable/geist-mono';
import './home.css';
import { baseMetadata } from '@/lib/seo';

export const metadata = baseMetadata;

/**
 * The landing's own root layout.
 *
 * It is a second root (the docs, blog and changelog live under `(site)`) so
 * the landing can speak the product's design system — `@rustrak/ui`'s tokens,
 * Inter, the charcoal surfaces — without its `:root` variables colliding with
 * the docs theme's `--border`, `--surface` and `--radius-*`. Crossing between
 * the two roots is a full page load, which is what keeps the two stylesheets
 * from ever being on one page.
 */
export default function HomeLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en" data-theme="dark">
      <body>{children}</body>
    </html>
  );
}
