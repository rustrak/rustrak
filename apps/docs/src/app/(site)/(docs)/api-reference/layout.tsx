import type { Metadata } from 'next';

/** The page is a client component, which cannot export metadata itself. */
export const metadata: Metadata = {
  title: 'API Reference',
  description:
    'Interactive reference for the Rustrak REST API: every endpoint, parameter, schema and authentication method, generated from the OpenAPI spec.',
  alternates: { canonical: '/api-reference' },
};

export default function ApiReferenceLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return children;
}
