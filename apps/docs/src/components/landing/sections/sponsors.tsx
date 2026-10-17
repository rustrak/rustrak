import { Heart } from 'lucide-react';
import { Band, Cell } from '@/components/frame/grid';
import { Heading, Pill } from '../primitives/heading';
import { fetchSponsors, type Sponsor } from '../sponsors';

const SPONSOR_LINKS = [
  { href: 'https://github.com/sponsors/AbianS', label: 'Sponsor on GitHub' },
  { href: 'https://opencollective.com/rustrak', label: 'Open Collective' },
];

/**
 * Column counts per sponsor count, as literal class strings because Tailwind
 * scans source text and never sees a name built from a variable.
 *
 * Two columns is the ceiling: this grid lives in half the page, and a third
 * column there would shrink each cell below the size an avatar and a login
 * need. One sponsor gets the whole box to itself rather than a lonely cell
 * with an empty one beside it.
 */
const LAYOUTS: Record<number, string> = {
  1: 'grid-cols-1',
  2: 'grid-cols-1 sm:grid-cols-2',
};

const FALLBACK_LAYOUT = 'grid-cols-1 sm:grid-cols-2';

/**
 * The sponsor wall. An async Server Component, so the fetch happens once at
 * build and the exported HTML already contains the people in it.
 *
 * Renders nothing at all when there is nobody to thank. An empty wall with a
 * "be the first" placeholder is worse than no wall: it advertises that nobody
 * has, which is the opposite of what the section is for.
 *
 * The wall is built as ruled cells like the SDK strip, so it belongs to the
 * same page rather than introducing a third way of presenting a list. The
 * rules come from a 1px grid gap over a rule-coloured backing rather than from
 * borders on each cell: the lines then fall between cells automatically at any
 * column count, with no per-cell arithmetic to get wrong as the number of
 * sponsors changes.
 */
export async function Sponsors() {
  const sponsors = await fetchSponsors();
  if (sponsors.length === 0) return null;

  const count = sponsors.length;
  const gold = sponsors.filter((sponsor) => sponsor.level === 'gold');
  const cards = sponsors.filter((sponsor) => sponsor.level === 'sponsor');
  const backers = sponsors.filter((sponsor) => sponsor.level === 'backer');
  const layout = LAYOUTS[cards.length] ?? FALLBACK_LAYOUT;
  // Completes the last row so the backing never shows through as a block.
  const fillers = cards.length > 2 && cards.length % 2 === 1 ? 1 : 0;

  return (
    <Band>
      <div className="grid lg:grid-cols-2">
        <Cell className="flex flex-col justify-center">
          <div>
            <Pill>Sponsors</Pill>
            {/*
              Thanks, and nothing else.

              It used to read "Rustrak is free, GPL-3.0 and self-hosted. These
              people pay for that.", which made two claims the section should not
              be making. The licence belongs to the closing band, and sponsorship
              is not what funds the project: it helps, and overstating it turns a
              thank-you into a pitch.
            */}
            <Heading
              className="display-lg mt-6 max-w-[18ch]"
              lead="Thank you."
              rest="To everyone sponsoring Rustrak."
              scrub
            />
          </div>

          <div className="mt-10 flex flex-wrap gap-3">
            {SPONSOR_LINKS.map(({ href, label }) => (
              <a
                key={href}
                href={href}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex w-fit items-center gap-2 rounded-lg border border-white/14 px-4 py-2.5 text-[14px] font-medium text-white/85 transition-colors hover:border-white/28 hover:text-white"
              >
                <Heart className="size-4 text-primary" />
                {label}
              </a>
            ))}
          </div>
        </Cell>

        <div className="flex flex-col border-t border-rule lg:border-l lg:border-t-0">
          <p className="border-b border-rule px-5 py-4 font-mono text-[10.5px] uppercase tracking-[0.18em] text-white/30 sm:px-10">
            {count === 1 ? '1 sponsor' : `${count} sponsors`}
          </p>

          {/* Gold Sponsors: one per row, the largest card on the wall. */}
          {gold.length > 0 && (
            <ul className="grid grid-cols-1 gap-px border-b border-rule bg-[var(--rule)]">
              {gold.map((sponsor) => (
                <li key={sponsor.url} className="bg-[var(--surface)]">
                  <SponsorCard sponsor={sponsor} large />
                </li>
              ))}
            </ul>
          )}

          {/* `flex-1` lets the grid take whatever height the copy column sets,
              so a single sponsor fills its box instead of sitting at the top
              of a tall empty one. */}
          {cards.length > 0 && (
            <ul className={`grid flex-1 gap-px bg-[var(--rule)] ${layout}`}>
              {cards.map((sponsor) => (
                <li key={sponsor.url} className="bg-[var(--surface)]">
                  <SponsorCard sponsor={sponsor} />
                </li>
              ))}

              {Array.from({ length: fillers }, (_, index) => (
                /* Purely structural: keeps the backing from showing as a block.
                   Only at the two-column breakpoint, because in one column there
                   is no half-row left to complete — the filler is a whole extra
                   row there, and with the grid stretching its auto rows it shows
                   up as a full empty cell with a rule above it. */
                <li
                  key={`filler-${index}`}
                  aria-hidden
                  className="hidden bg-[var(--surface)] sm:block"
                />
              ))}
            </ul>
          )}

          {/* Supporters and Backers: a name each, no card. */}
          {backers.length > 0 && (
            <ul className="flex flex-wrap gap-x-5 gap-y-3 border-t border-rule px-5 py-6 sm:px-10">
              {backers.map((sponsor) => (
                <li key={sponsor.url}>
                  <a
                    href={sponsor.url}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="group inline-flex items-center gap-2 text-[13px] text-white/60 transition-colors hover:text-white"
                  >
                    <Avatar sponsor={sponsor} size={20} className="size-5" />
                    {sponsor.name}
                  </a>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </Band>
  );
}

function SponsorCard({
  sponsor,
  large = false,
}: {
  sponsor: Sponsor;
  large?: boolean;
}) {
  return (
    <a
      href={sponsor.url}
      target="_blank"
      rel="noopener noreferrer"
      className={`group flex h-full flex-col items-center justify-center gap-4 px-5 text-center transition-colors hover:bg-white/3 sm:px-6 ${
        large ? 'py-14 sm:py-16' : 'py-10 sm:py-12'
      }`}
    >
      <Avatar
        sponsor={sponsor}
        size={large ? 96 : 64}
        className={`ring-1 ring-white/12 transition-[box-shadow] group-hover:ring-primary/40 ${
          large ? 'size-24' : 'size-16'
        }`}
      />

      <span>
        <span
          className={`block text-white/85 transition-colors group-hover:text-white ${
            large ? 'text-[19px]' : 'text-[15px]'
          }`}
        >
          {sponsor.name}
        </span>
        <span className="mt-1 block font-mono text-[10.5px] uppercase tracking-[0.16em] text-white/30">
          {large
            ? 'Gold Sponsor'
            : sponsor.isOrganization
              ? 'Organization'
              : 'Individual'}
        </span>
      </span>
    </a>
  );
}

function Avatar({
  sponsor,
  size,
  className,
}: {
  sponsor: Sponsor;
  size: number;
  className: string;
}) {
  return (
    /* Static export with images unoptimized: a plain img is what next/image
       would emit here anyway. */
    // biome-ignore lint/performance/noImgElement: static export with unoptimized images, so next/image emits a plain img here anyway
    <img
      src={sponsor.avatar}
      alt=""
      width={size}
      height={size}
      loading="lazy"
      decoding="async"
      className={`${className} ${sponsor.isOrganization ? 'rounded-xl' : 'rounded-full'}`}
    />
  );
}
