import { cn, Wordmark } from '@rustrak/ui';
import { CONTAINER, GITHUB, LINKS } from './site';

/*
 * A Server Component, apart from `chrome.tsx` on purpose: that module is
 * `'use client'` and shares a chunk with the landing, so importing the footer
 * from it made every docs page download the landing's product mockup.
 */
export function Footer() {
  return (
    <footer className="border-border-subtle border-t">
      <div
        className={cn(
          CONTAINER,
          'flex flex-col gap-8 py-12 md:flex-row md:items-center md:justify-between',
        )}
      >
        <div className="flex flex-col gap-3">
          <Wordmark still className="h-5 w-auto self-start text-fg" />
          <p className="text-meta text-fg-meta">
            AGPL-3.0 · Not affiliated with Sentry. Compatible with its SDKs.
          </p>
        </div>
        <ul className="flex flex-wrap gap-6">
          {[
            ...LINKS,
            { href: '/vs/sentry', label: 'vs Sentry' },
            { href: GITHUB, label: 'GitHub' },
          ].map((link) => (
            <li key={link.href}>
              <a
                href={link.href}
                className="text-control text-fg-subtle transition-colors hover:text-fg"
              >
                {link.label}
              </a>
            </li>
          ))}
        </ul>
      </div>
    </footer>
  );
}
