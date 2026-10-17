import Link from 'next/link';
import { RustrakWordmark } from '@/components/icons/rustrak-wordmark';
import { GITHUB } from '../links';

// Open Collective rather than GitHub Sponsors: it lists every tier, takes
// one-off donations and shows where the money goes.
const SPONSOR_URL = 'https://opencollective.com/rustrak';

/**
 * Four columns, because four divides into two and into four.
 *
 * The rules between these cells are drawn only where a cell has a neighbour, so
 * the grid closes flush against the frame instead of doubling its lines — and
 * that only stays true if the count fits both layouts without an orphan. It is
 * the same arithmetic the SDK strip and the footprint numbers depend on, and it
 * is the reason the outbound links are a fourth group rather than a stray pair
 * dropped under the third.
 */
const COLUMNS = [
  {
    title: 'Docs',
    links: [
      { href: '/getting-started/overview', label: 'Overview' },
      { href: '/getting-started/installation', label: 'Installation' },
      { href: '/getting-started/quickstart', label: 'Quickstart' },
      { href: '/api-reference', label: 'API reference' },
    ],
  },
  {
    title: 'Configuration',
    links: [
      { href: '/configuration/environment', label: 'Environment' },
      { href: '/configuration/database', label: 'Database' },
      { href: '/configuration/production', label: 'Production' },
    ],
  },
  {
    title: 'Project',
    links: [
      { href: '/roadmap', label: 'Roadmap' },
      { href: '/changelog', label: 'Changelog' },
      { href: '/blog', label: 'Blog' },
      { href: GITHUB, label: 'GitHub' },
    ],
  },
  /*
    Named for what a reader wants from it, not for what they can give: as
    "Support" over `Sponsor` and `Contributing`, somebody arriving with a broken
    deploy found two invitations to help and no way to ask for any. Discussions
    is deliberately absent — it is switched off on the repository.
  */
  {
    title: 'Community',
    links: [
      { href: `${GITHUB}/issues`, label: 'Issues' },
      { href: '/reference/contributing', label: 'Contributing' },
      { href: SPONSOR_URL, label: 'Sponsor' },
    ],
  },
] as const;

/**
 * The footer, uncovered by the page sliding over it.
 *
 * There is deliberately no giant wordmark. One was tried — `RUSTRAK` at 320px,
 * bled to the frame's rules — and it is four and a half times the largest type
 * anywhere else here, so it reads as a different page's masthead arriving just
 * as this one is meant to hand over quietly. It also cost 230px of height, most
 * of what a footer should measure in total. See the display scale note in
 * `globals.css`. The mark is the nav's, at the nav's size.
 *
 * No rule along the top, and that is not an omission: the closing band already
 * closes against its own bottom hairline, so a second here would double it
 * where the two meet. During the reveal that hairline is the moving edge, and
 * this footer has no edge of its own until it is uncovered.
 *
 * The reveal is the lid idea again, by a different mechanism, and the
 * difference is why this is not a `Pinned`. `Pinned` sticks to the *top* of the
 * viewport, which forces its band to be a full screen tall — right for one
 * sentence over a fresco, wrong for a footer. Sticking to the *bottom* removes
 * the constraint: the footer is whatever height its contents make it, and the
 * closing band above covers it until it does not (see `Cover` in
 * `landing-page.tsx`). The arithmetic falls out — scroll up by δ and exactly δ
 * less of the footer shows — so there is no corridor to pad and no two numbers
 * to keep in agreement.
 *
 * It cannot get this wrong, either: a sticky element is clamped by its
 * containing block, which here is the box holding the closing band *and* this
 * footer, so the footer physically cannot be pulled above the closing's top
 * edge. A window too short simply ends the reveal early.
 *
 * Below `sm` this is an ordinary block at the end of the document. Uncovering a
 * footer is something you notice steering a wheel; with a thumb it reads as a
 * footer that lags.
 */
export function SiteFooter() {
  return (
    <footer className="sm:sticky sm:bottom-0">
      {/*
        Two up on a phone, four across from `lg`, and the rules are drawn only
        where a cell has a neighbour. Each case is spelled out as a literal
        class: Tailwind scans source text, so a class name assembled from a
        variable prefix is never generated. This is the footprint strip's
        pattern exactly, because it is the same shape of problem.
      */}
      <ul className="grid grid-cols-2 lg:grid-cols-4">
        {COLUMNS.map((column, index) => (
          <li
            key={column.title}
            className={[
              'border-rule px-5 py-8 sm:px-10 sm:py-10',
              index === 1 ? 'lg:border-r' : index === 3 ? '' : 'border-r',
              index < 2 ? 'border-b lg:border-b-0' : '',
            ].join(' ')}
          >
            <p className="eyebrow">{column.title}</p>
            <ul className="mt-4">
              {column.links.map((link) => (
                <li key={link.href}>
                  {/* `block` with vertical padding rather than a bare inline
                      link: at 13.5px the hit area was a third of the 44px a
                      thumb needs, and these sit close enough together to make
                      that a real miss. */}
                  <Link
                    href={link.href}
                    className="block py-1.5 text-[13.5px] text-white/55 transition-colors hover:text-white"
                  >
                    {link.label}
                  </Link>
                </li>
              ))}
            </ul>
          </li>
        ))}
      </ul>

      {/* The only place a wordmark belongs in this footer: the nav's lockup at
          the nav's size, so the page opens and closes on the same mark, and at
          the head of the licence line it costs no height — the row was already
          there and its left edge was doing nothing. */}
      <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2 border-t border-rule px-5 py-6 text-[12.5px] text-white/35 sm:gap-3 sm:px-10">
        <p className="flex items-center gap-2.5">
          <RustrakWordmark className="h-3.5 w-auto text-white/45" decorative />
          {/* `Rustrak` as text here is the one case the brand allows: inside a
              sentence it is a word, not the mark. The mark is the drawing beside
              it.

              Which is also why the drawing is `decorative`: the word is already
              said, in the sentence, so a labelled mark in front of it announces
              "Rustrak, Rustrak · GPL-3.0". This is the only placement on the
              page where that is true. */}
          <span>Rustrak · GPL-3.0</span>
        </p>
        <p>Not affiliated with Sentry. Compatible with its SDKs.</p>
      </div>
    </footer>
  );
}
