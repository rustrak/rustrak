import type { Metadata } from 'next';
import { Home } from '@/components/home/home';
import { GITHUB } from '@/components/home/site';
import { baseMetadata, DEFINITION, jsonLd, SITE } from '@/lib/seo';

/**
 * Neither the title nor the description claims parity with Sentry:
 * `rustrak-brand`'s voice guide rules that phrasing out by name. They claim
 * compatibility with the SDKs, which is true, and nothing more.
 */
const title =
  'Rustrak: open-source, self-hosted error tracking for Sentry SDKs';
const description =
  'Self-hosted error tracking compatible with the official Sentry SDKs. One Rust container on SQLite or PostgreSQL, under 60 MB of memory. Open source, AGPL-3.0.';

export const metadata: Metadata = {
  title,
  description,
  alternates: { canonical: '/' },
  openGraph: { ...baseMetadata.openGraph, title, description, url: '/' },
};

/**
 * Who publishes the site and what the product is, for search engines and
 * assistants that read structured data. `sameAs` ties the brand to its GitHub
 * and Docker Hub pages, so "rustrak" resolves to one entity.
 */
const structuredData = {
  '@context': 'https://schema.org',
  '@graph': [
    {
      '@type': 'Organization',
      '@id': `${SITE}/#organization`,
      name: 'Rustrak',
      url: SITE,
      logo: `${SITE}/icon.png`,
      sameAs: [GITHUB, 'https://hub.docker.com/r/rustrak/rustrak-server'],
    },
    {
      '@type': 'WebSite',
      '@id': `${SITE}/#website`,
      name: 'Rustrak',
      url: SITE,
      publisher: { '@id': `${SITE}/#organization` },
    },
    {
      '@type': 'SoftwareApplication',
      name: 'Rustrak',
      description: DEFINITION,
      url: SITE,
      applicationCategory: 'DeveloperApplication',
      operatingSystem: 'Linux, Docker',
      license: 'https://www.gnu.org/licenses/agpl-3.0.html',
      offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
      downloadUrl: 'https://hub.docker.com/r/rustrak/rustrak-server',
      codeRepository: GITHUB,
      publisher: { '@id': `${SITE}/#organization` },
    },
  ],
};

export default function Page() {
  return (
    <>
      <script {...jsonLd(structuredData)} />
      <Home />
    </>
  );
}
