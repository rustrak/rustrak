'use client';

import {
  Button,
  CopyIcon,
  cn,
  SegmentedControl,
  SegmentedItem,
  SelectedIcon,
} from '@rustrak/ui';
import { useState } from 'react';
import { SectionHead } from './chrome';
import { Showcase } from './features';
import { Reveal } from './reveal';
import { CONTAINER } from './site';

/** Verbatim from content/getting-started/installation.mdx. */
const COMMANDS = {
  sqlite: [
    'docker run -d \\',
    '  --name rustrak \\',
    '  -p 8080:8080 \\',
    '  -v rustrak_data:/data \\',
    '  -e SESSION_SECRET_KEY=$(openssl rand -hex 32) \\',
    '  -e CREATE_SUPERUSER=admin@example.com:changeme123 \\',
    '  rustrak/rustrak-server:latest',
  ],
  postgres: [
    'docker run -d \\',
    '  --name rustrak \\',
    '  -p 8080:8080 \\',
    '  -e DATABASE_URL="postgres://user:pass@db-host:5432/rustrak" \\',
    '  -e SESSION_SECRET_KEY="$(openssl rand -hex 32)" \\',
    '  -e CREATE_SUPERUSER="admin@example.com:changeme123" \\',
    '  rustrak/rustrak-server:postgres',
  ],
};

type Db = keyof typeof COMMANDS;

/**
 * Measured, not promised: README's burst test, 20,000 events on a 4-core box
 * with SQLite.
 */
const STATS = [
  { value: '<60 MB', label: 'Peak memory under a 20,000-event burst' },
  { value: '5k/s', label: 'Events accepted per second on 4 cores' },
  { value: '1', label: 'Process for the API, ingestion and dashboard' },
];

export function Deploy() {
  const [db, setDb] = useState<Db>('sqlite');
  // The command that was copied, not a flag: a copy that lands after the tab
  // changed then confirms nothing on the tab now shown.
  const [copiedText, setCopiedText] = useState<string | null>(null);
  const text = COMMANDS[db].join('\n');
  const copied = copiedText === text;

  const copy = () => {
    // `clipboard` is missing outside secure contexts.
    navigator.clipboard?.writeText(text).then(
      () => {
        setCopiedText(text);
        setTimeout(
          () => setCopiedText((current) => (current === text ? null : current)),
          1600,
        );
      },
      () => {},
    );
  };

  return (
    <section className="py-24 sm:py-32">
      <div className={cn(CONTAINER, 'grid items-center gap-14 lg:grid-cols-2')}>
        <div className="flex flex-col gap-12">
          <SectionHead
            eyebrow="06 Deploy"
            title="Up in one command."
            lead="One container, SQLite by default or PostgreSQL when you want it. The first boot creates your admin user, and the dashboard is served from the same port."
          />
          <dl className="grid grid-cols-3 gap-6 border-border-subtle border-t pt-8">
            {STATS.map((stat, i) => (
              <Reveal
                key={stat.label}
                delay={0.45 + i * 0.1}
                className="flex flex-col gap-1.5"
              >
                <dd className="order-1 text-page-title text-fg tabular-nums">
                  {stat.value}
                </dd>
                <dt className="order-2 text-meta text-fg-meta">{stat.label}</dt>
              </Reveal>
            ))}
          </dl>
        </div>

        <Showcase designWidth={620} interactive className="aspect-[5/4]">
          <div className="flex items-center justify-between gap-3 border-border-divider border-b px-4 py-3">
            <SegmentedControl
              aria-label="Database"
              value={db}
              onValueChange={(value) => setDb(value as Db)}
            >
              <SegmentedItem value="sqlite">SQLite</SegmentedItem>
              <SegmentedItem value="postgres">PostgreSQL</SegmentedItem>
            </SegmentedControl>
            <Button
              variant="ghost"
              size="sm"
              icon={copied ? SelectedIcon : CopyIcon}
              onClick={copy}
            >
              {copied ? 'Copied' : 'Copy'}
            </Button>
          </div>
          <pre className="flex-1 overflow-x-auto bg-code px-5 py-5 font-mono text-mono-sm leading-loose text-code-fg">
            <span className="text-fg-ghost">$ </span>
            {text}
          </pre>
        </Showcase>
      </div>
    </section>
  );
}
