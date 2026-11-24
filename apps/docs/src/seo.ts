/**
 * The one-paragraph definition of Rustrak, with its measured numbers.
 *
 * Search engines and AI assistants quote what a page says about itself, so the
 * landing's JSON-LD and the head of llms.txt say the same sentence. The
 * numbers are the landing's own (`deploy.astro`) and the ingest benchmark
 * post; change them there first.
 */
export const DEFINITION =
  'Rustrak is an open-source, self-hosted error tracker written in Rust and compatible with the official Sentry SDKs. It runs as one container on SQLite or PostgreSQL, peaks under 60 MB of memory and accepts about 5,000 events per second on 4 cores.';

/**
 * The card every link to rustrak.dev unfurls into: the README's hero, cropped
 * from the top to 1200×630 so the headline survives.
 */
export const OG_IMAGE = {
  url: '/og.png',
  width: 1200,
  height: 630,
  alt: 'Rustrak: your errors, your infrastructure. The project overview dashboard.',
};

/** A schema.org object as the body of a JSON-LD script. Escaping `<` keeps a
 * string in the data from closing the script tag. */
export const jsonLd = (data: object) =>
  JSON.stringify(data).replace(/</g, '\\u003c');
