import type { Metadata } from 'next';

export const metadata: Metadata = {
  title: {
    default: 'Blog',
    template: '%s — Rustrak Blog',
  },
  description:
    'Updates, releases, and engineering notes from the Rustrak project.',
};

export default function BlogLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return <div className="min-h-screen w-full">{children}</div>;
}
