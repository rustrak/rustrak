'use client';

import { Button, cn, StarIcon, Wordmark } from '@rustrak/ui';
import * as m from 'motion/react-m';
import Link from 'next/link';
import { useEffect, useState } from 'react';
import { EASE } from '../landing/motion';
import { Reveal, Words } from './reveal';
import { CONTAINER, GITHUB, START } from './site';

const LINKS = [
  { href: '/getting-started/overview', label: 'Docs' },
  { href: '/roadmap', label: 'Roadmap' },
  { href: '/changelog', label: 'Changelog' },
  { href: '/blog', label: 'Blog' },
];

export function Nav() {
  const [scrolled, setScrolled] = useState(false);
  useEffect(() => {
    const update = () => setScrolled(window.scrollY > 8);
    update();
    window.addEventListener('scroll', update, { passive: true });
    return () => window.removeEventListener('scroll', update);
  }, []);

  return (
    <m.header
      initial={{ opacity: 0, y: -16 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 1, ease: EASE, delay: 0.1 }}
      className={cn(
        'fixed inset-x-0 top-0 z-50 border-b transition-[background-color,border-color] duration-slow ease-standard',
        scrolled
          ? 'border-border-subtle bg-canvas/80 backdrop-blur-xl'
          : 'border-transparent',
      )}
    >
      <nav className={cn(CONTAINER, 'flex h-16 items-center gap-8')}>
        <Link href="/" aria-label="Rustrak home">
          <Wordmark className="h-5 w-auto text-fg" />
        </Link>
        <ul className="hidden items-center gap-6 md:flex">
          {LINKS.map((link) => (
            <li key={link.href}>
              <Link
                href={link.href}
                className="text-control text-fg-subtle transition-colors hover:text-fg"
              >
                {link.label}
              </Link>
            </li>
          ))}
        </ul>
        <div className="ms-auto flex items-center gap-2">
          {/* Below md the links fold into a native disclosure. */}
          <details className="relative md:hidden">
            <summary className="cursor-pointer list-none px-2 text-control text-fg-subtle hover:text-fg">
              Menu
            </summary>
            <ul className="absolute right-0 mt-3 flex min-w-40 flex-col rounded-xl border border-border-subtle bg-panel p-2 shadow-dialog">
              {LINKS.map((link) => (
                <li key={link.href}>
                  <Link
                    href={link.href}
                    className="block rounded-md px-3 py-2 text-control text-fg-subtle hover:bg-canvas hover:text-fg"
                  >
                    {link.label}
                  </Link>
                </li>
              ))}
            </ul>
          </details>
          <Button
            variant="ghost"
            size="sm"
            icon={StarIcon}
            className="max-sm:hidden"
            /* react-doctor-disable-next-line react-doctor/anchor-has-content */
            render={<a href={GITHUB} />}
          >
            GitHub
          </Button>
          {/* react-doctor-disable-next-line react-doctor/anchor-has-content */}
          <Button variant="primary" size="sm" render={<a href={START} />}>
            Get started
          </Button>
        </div>
      </nav>
    </m.header>
  );
}

/** Eyebrow, headline and one paragraph: the opening of every section. */
export function SectionHead({
  eyebrow,
  title,
  lead,
  center,
}: {
  eyebrow: string;
  title: string;
  lead: string;
  center?: boolean;
}) {
  return (
    <div
      className={cn(
        'flex max-w-2xl flex-col gap-5',
        center && 'mx-auto items-center text-center',
      )}
    >
      <Reveal y={12}>
        <p className="font-mono text-mono text-fg-brand uppercase tracking-wider">
          {eyebrow}
        </p>
      </Reveal>
      <Words
        text={title}
        delay={0.1}
        className="text-balance text-headline text-fg"
      />
      {lead ? (
        <Reveal delay={0.35}>
          <p className="text-pretty text-lead text-fg-subtle">{lead}</p>
        </Reveal>
      ) : null}
    </div>
  );
}

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
          {[...LINKS, { href: GITHUB, label: 'GitHub' }].map((link) => (
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
