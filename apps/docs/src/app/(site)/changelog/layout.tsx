import type { Metadata } from 'next';

export const metadata: Metadata = {
  title: {
    default: 'Changelog',
    template: '%s — Rustrak Changelog',
  },
  description: 'Release notes and version history for the Rustrak project.',
};

export default function ChangelogLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return <div className="min-h-screen w-full">{children}</div>;
}
