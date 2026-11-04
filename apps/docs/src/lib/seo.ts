import type { Metadata } from 'next';

export const SITE = 'https://rustrak.dev';

/**
 * The one-paragraph definition of Rustrak, with its measured numbers.
 *
 * Search engines and AI assistants quote what a page says about itself, so the
 * landing's JSON-LD and the head of llms.txt say the same sentence, and the
 * landing's meta description is its short form. The numbers are the landing's own (`deploy.tsx`) and the
 * ingest benchmark post; change them there first.
 */
export const DEFINITION =
  'Rustrak is an open-source, self-hosted error tracker written in Rust and compatible with the official Sentry SDKs. It runs as one container on SQLite or PostgreSQL, peaks under 60 MB of memory and accepts about 5,000 events per second on 4 cores.';

/**
 * What both root layouts share. There are two roots (`(home)` and `(site)`),
 * so neither inherits from the other and each has to spread this.
 *
 * `metadataBase` turns every relative URL below and in the pages (canonical,
 * Open Graph images) into an absolute one. The canonical itself is set per
 * page: a relative one in a layout would point every child at the layout.
 */
/**
 * The card every link to rustrak.dev unfurls into: the README's hero, cropped
 * from the top to 1200×630 so the headline survives. A static PNG rather than an
 * `opengraph-image` route: the static export writes that route without a file
 * extension, and GitHub Pages would serve it as `application/octet-stream`.
 * Next merges `openGraph` shallowly, so a page that sets its own has to list
 * this again.
 */
export const OG_IMAGE = {
  url: '/og.png',
  width: 1200,
  height: 630,
  alt: 'Rustrak: your errors, your infrastructure. The project overview dashboard.',
};

export const baseMetadata: Metadata = {
  metadataBase: new URL(SITE),
  openGraph: {
    siteName: 'Rustrak',
    type: 'website',
    locale: 'en_US',
    images: [OG_IMAGE],
  },
  twitter: { card: 'summary_large_image', images: [OG_IMAGE.url] },
};

/** `<script type="application/ld+json">` for a schema.org object. */
export function jsonLd(data: object) {
  return {
    type: 'application/ld+json',
    // Escaping `<` keeps a string in the data from closing the script tag.
    dangerouslySetInnerHTML: {
      __html: JSON.stringify(data).replace(/</g, '\\u003c'),
    },
  };
}
