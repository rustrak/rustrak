'use client';

import * as m from 'motion/react-m';
import Link from 'next/link';
import { GithubIcon } from '@/components/icons/github';
import { RustrakWordmark } from '@/components/icons/rustrak-wordmark';
import { cn } from '@/lib/utils';
import { DUR, EASE, STAGGER } from '../motion';
import { HANDHELD, useMediaQuery } from '../use-media-query';
import { useStarted } from '../use-started';
import { LINKS, REPO } from './links';
import { MobileMenu } from './mobile-menu';
import { useMobileMenu } from './use-mobile-menu';
import { useNavScroll } from './use-nav-scroll';

/** The bar drops in as one piece, then its contents arrive left to right. */
const BAR_VARIANTS = {
  hidden: { y: -72, opacity: 0 },
  visible: {
    y: 0,
    opacity: 1,
    transition: {
      duration: DUR.slow,
      ease: EASE,
      delay: 0.5,
      staggerChildren: STAGGER,
      delayChildren: 0.75,
    },
  },
};

const ITEM_VARIANTS = {
  hidden: { y: -12, opacity: 0 },
  visible: { y: 0, opacity: 1, transition: { duration: DUR.base, ease: EASE } },
};

export function LandingNav() {
  const started = useStarted();
  const handheld = useMediaQuery(HANDHELD);
  const { lifted, tucked, untuck } = useNavScroll();
  const menu = useMobileMenu({ onOpen: untuck });

  /** Out of the way only where it can be brought back: a phone, menu closed. */
  const away = handheld && tucked && !menu.open;

  /*
    Whether the overlay is on the screen, which `menu.open` does not answer.

    Closing is a fade, so the panel goes on being painted, and covering the
    page, for `DUR.fast` after the flag has gone. Everything that is really
    about "is this surface in the way" hangs off this rather than off the flag:
    the scroll lock, or a swipe during the fade scrolls the landing behind a
    menu still covering it; and the keyboard, for the reason below.
  */ return (
    <>
      {/*
        Three elements, one transform each, and that separation is the reason
        this is not one.

        The entrance is authored as variants on the header, and the parent
        variant is what staggers the contents in. Putting the tuck on the same
        element would mean re-entering the `visible` variant every time the bar
        came back — re-running the child stagger, and re-running it against
        whatever the entrance was doing if the reader scrolled during the first
        second of the page. A separate layer for the tuck cannot collide with
        any of that, and the outer box is what stays fixed while both of them
        move inside it.
      */}
      <div
        className="fixed inset-x-0 top-0 z-40"
        /* A tucked bar is off screen but its links are still in the tab order,
           and focusing something above the viewport is a dead end. Cheaper and
           kinder than `inert`: reaching for it brings it back. */
        onFocusCapture={untuck}
      >
        <m.div
          animate={{ y: away ? '-102%' : '0%' }}
          /* Just past 100%, so the border under the bar clears the top edge
             instead of leaving a hairline stuck to it. */
          transition={{ duration: DUR.base, ease: EASE }}
        >
          <m.header
            initial="hidden"
            animate={started ? 'visible' : 'hidden'}
            variants={BAR_VARIANTS}
            className={cn(
              'transition-colors duration-300',
              lifted
                ? 'border-b border-white/8 bg-[oklch(0.14_0_0)]/72 backdrop-blur-xl'
                : 'border-b border-transparent',
            )}
          >
            <nav className="mx-auto flex h-16 max-w-360 items-center justify-between px-5 sm:px-6 md:px-10">
              <m.div variants={ITEM_VARIANTS}>
                <Link
                  href="/"
                  className="flex items-center focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary"
                >
                  {/* The mark, placed rather than typed.

                      This was the bolt tile beside the word set in `font-semibold
                      uppercase tracking-[0.16em]`, and the brand rules that out by
                      name: typed, the mark depends on which font the browser
                      resolved, at which weight, and on the network not failing. The
                      only place `rustrak` is written as text is inside a prose
                      sentence, where it is a word and not the mark.

                      18px, not the 24 the box wants. The artwork is trimmed to the
                      ink, so an 18px wordmark has 18px letters while 18px *text*
                      beside it has letters of about 13 — matched by box height the
                      mark always over-powers its neighbours. */}
                  <RustrakWordmark className="h-[18px] w-auto text-foreground" />
                </Link>
              </m.div>

              <div className="hidden items-center gap-8 md:flex">
                {LINKS.map((link) => (
                  <m.div key={link.href} variants={ITEM_VARIANTS}>
                    <Link
                      href={link.href}
                      className="text-[13px] text-white/55 transition-colors hover:text-white focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary"
                    >
                      {link.label}
                    </Link>
                  </m.div>
                ))}
                <m.div variants={ITEM_VARIANTS}>
                  <Link
                    href={REPO}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="flex items-center gap-1.5 text-[13px] text-white/55 transition-colors hover:text-white focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary"
                  >
                    <GithubIcon className="size-3.5" />
                    GitHub
                  </Link>
                </m.div>
                <m.div variants={ITEM_VARIANTS}>
                  <Link
                    href="/getting-started/installation"
                    className="rounded-full bg-white px-4 py-1.5 text-[13px] font-medium text-black transition-opacity hover:opacity-85 focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary"
                  >
                    Get started
                  </Link>
                </m.div>
              </div>

              {/* Pulled out into its own margin so the 44px hit area a thumb needs
              does not push the bar's contents apart to get it. */}
              <m.button
                ref={menu.buttonRef}
                variants={ITEM_VARIANTS}
                type="button"
                onClick={menu.openMenu}
                aria-label="Open menu"
                aria-expanded={menu.open}
                className="-mr-3 flex flex-col items-center gap-[5px] p-3 md:hidden"
              >
                <span className="block h-px w-5 bg-white" />
                <span className="block h-px w-5 bg-white" />
              </m.button>
            </nav>
          </m.header>
        </m.div>
      </div>
      <MobileMenu
        open={menu.open}
        panelRef={menu.panelRef}
        onClose={menu.closeMenu}
        onGone={menu.onGone}
      />
    </>
  );
}
