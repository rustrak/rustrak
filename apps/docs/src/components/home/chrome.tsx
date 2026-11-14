'use client';

import { Button, CloseIcon, cn, MenuIcon, Wordmark } from '@rustrak/ui';
import { AnimatePresence } from 'motion/react';
import * as m from 'motion/react-m';
import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { useEffect, useRef, useState } from 'react';
import { EASE } from '../landing/motion';
import { Reveal, Words } from './reveal';
import { CONTAINER, GITHUB, START } from './site';

const LINKS = [
  { href: '/getting-started/overview', label: 'Docs' },
  { href: '/roadmap', label: 'Roadmap' },
  { href: '/changelog', label: 'Changelog' },
  { href: '/blog', label: 'Blog' },
  { href: '/benchmarks', label: 'Benchmarks' },
];

const GITHUB_MARK =
  'M12 .5C5.73.5.5 5.74.5 12.04c0 5.1 3.29 9.42 7.86 10.95.58.1.79-.25.79-.56v-2c-3.2.7-3.87-1.37-3.87-1.37-.52-1.33-1.28-1.69-1.28-1.69-1.04-.72.08-.7.08-.7 1.15.08 1.76 1.19 1.76 1.19 1.03 1.76 2.69 1.25 3.35.96.1-.75.4-1.25.73-1.54-2.55-.29-5.24-1.28-5.24-5.69 0-1.26.45-2.29 1.18-3.1-.12-.29-.51-1.46.11-3.05 0 0 .97-.31 3.17 1.18a11 11 0 0 1 5.77 0c2.2-1.49 3.17-1.18 3.17-1.18.63 1.59.23 2.76.11 3.05.74.81 1.18 1.84 1.18 3.1 0 4.42-2.7 5.39-5.26 5.68.41.36.78 1.06.78 2.14v3.17c0 .31.21.67.8.56A11.5 11.5 0 0 0 23.5 12.04C23.5 5.74 18.27.5 12 .5Z';

/** GitHub's mark, sized and coloured like the icons the buttons draw. */
function GitHubMark() {
  return (
    <svg viewBox="0 0 24 24" className="size-4" fill="currentColor" aria-hidden>
      <path d={GITHUB_MARK} />
    </svg>
  );
}

/** The section a path belongs to: `/blog/x` is under Blog. */
const isCurrent = (path: string, href: string) =>
  path === href || path.startsWith(`${href}/`);

export function Nav() {
  const path = usePathname();
  const [open, setOpen] = useState(false);
  const [scrolled, setScrolled] = useState(false);
  // A link in the sheet navigates; the sheet closes with the page it opened on.
  useEffect(() => setOpen(false), [path]);
  // While the sheet is open the page under it stays put and out of reach of
  // the keyboard (`inert`), Escape closes it, and so does the window growing
  // past `md`, where the sheet and its toggle are hidden.
  const header = useRef<HTMLElement>(null);
  useEffect(() => {
    if (!open) return;
    const page = [...(header.current?.parentElement?.children ?? [])].filter(
      (el): el is HTMLElement =>
        el !== header.current && el instanceof HTMLElement,
    );
    const close = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false);
    const wide = window.matchMedia('(min-width: 48rem)');
    const grow = () => wide.matches && setOpen(false);
    document.documentElement.style.overflow = 'hidden';
    for (const el of page) el.inert = true;
    window.addEventListener('keydown', close);
    wide.addEventListener('change', grow);
    return () => {
      document.documentElement.style.overflow = '';
      for (const el of page) el.inert = false;
      window.removeEventListener('keydown', close);
      wide.removeEventListener('change', grow);
    };
  }, [open]);
  useEffect(() => {
    const update = () => setScrolled(window.scrollY > 8);
    update();
    window.addEventListener('scroll', update, { passive: true });
    return () => window.removeEventListener('scroll', update);
  }, []);

  return (
    <m.header
      ref={header}
      initial={{ opacity: 0, y: -16 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 1, ease: EASE, delay: 0.1 }}
      className={cn(
        'fixed inset-x-0 top-0 z-50 border-b transition-[background-color,border-color] duration-slow ease-standard',
        open
          ? 'border-border-subtle bg-canvas'
          : scrolled
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
                aria-current={isCurrent(path, link.href) ? 'page' : undefined}
                className="text-control text-fg-subtle transition-colors hover:text-fg aria-[current=page]:text-fg"
              >
                {link.label}
              </Link>
            </li>
          ))}
        </ul>
        <div className="ms-auto flex items-center gap-2">
          <Button
            variant="ghost"
            size="sm"
            className="max-sm:hidden"
            /* react-doctor-disable-next-line react-doctor/anchor-has-content */
            render={<a href={GITHUB} />}
          >
            <GitHubMark />
            GitHub
          </Button>
          {/* react-doctor-disable-next-line react-doctor/anchor-has-content */}
          <Button
            variant="primary"
            size="sm"
            // The open sheet carries its own, full-width.
            className={cn(open && 'max-md:hidden')}
            render={<a href={START} />}
          >
            Get started
          </Button>
          <button
            type="button"
            aria-label={open ? 'Close menu' : 'Open menu'}
            aria-expanded={open}
            aria-controls="site-menu"
            onClick={() => setOpen((v) => !v)}
            className="-me-2 grid size-10 place-items-center rounded-md text-fg-subtle transition-colors hover:text-fg md:hidden"
          >
            {open ? <CloseIcon size="xl" /> : <MenuIcon size="xl" />}
          </button>
        </div>
      </nav>
      {/* Below md the links open as a full-height sheet under the bar. */}
      <AnimatePresence>
        {open ? (
          <m.div
            id="site-menu"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.2, ease: 'easeOut' }}
            className="absolute inset-x-0 top-full flex h-[calc(100dvh-4rem)] flex-col overflow-y-auto overscroll-contain bg-canvas md:hidden"
          >
            <ul className={cn(CONTAINER, 'flex flex-col pt-6')}>
              {LINKS.map((link, i) => (
                <m.li
                  key={link.href}
                  initial={{ opacity: 0, y: 12 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{
                    duration: 0.45,
                    ease: EASE,
                    delay: 0.04 + i * 0.04,
                  }}
                >
                  <Link
                    href={link.href}
                    aria-current={
                      isCurrent(path, link.href) ? 'page' : undefined
                    }
                    className="flex items-center gap-3 py-3 text-[1.75rem] text-fg-subtle tracking-tight transition-colors hover:text-fg aria-[current=page]:text-fg"
                  >
                    {link.label}
                  </Link>
                </m.li>
              ))}
            </ul>
            <m.div
              initial={{ opacity: 0, y: 12 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.45, ease: EASE, delay: 0.28 }}
              className={cn(CONTAINER, 'mt-auto flex flex-col gap-3 pb-10')}
            >
              <Button
                variant="ghost"
                size="lg"
                /* react-doctor-disable-next-line react-doctor/anchor-has-content */
                render={<a href={GITHUB} />}
              >
                <GitHubMark />
                Star on GitHub
              </Button>
              {/* react-doctor-disable-next-line react-doctor/anchor-has-content */}
              <Button variant="primary" size="lg" render={<a href={START} />}>
                Get started
              </Button>
            </m.div>
          </m.div>
        ) : null}
      </AnimatePresence>
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
