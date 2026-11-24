import {
  Button,
  CopyIcon,
  SegmentedControl,
  SegmentedItem,
  SelectedIcon,
} from '@rustrak/ui';

/*
 * The install command, SQLite or PostgreSQL. Rendered at build time with both
 * commands and both states of the copy button in the HTML; `controls.ts`
 * switches between them.
 */

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

export function DeployShot() {
  return (
    <div data-switch className="flex h-full flex-col">
      <div className="flex items-center justify-between gap-3 border-border-divider border-b px-4 py-3">
        <SegmentedControl aria-label="Database" value="sqlite">
          <SegmentedItem value="sqlite">SQLite</SegmentedItem>
          <SegmentedItem value="postgres">PostgreSQL</SegmentedItem>
        </SegmentedControl>
        <span data-copy>
          <Button variant="ghost" size="sm" icon={CopyIcon}>
            Copy
          </Button>
          <Button variant="ghost" size="sm" icon={SelectedIcon} hidden>
            Copied
          </Button>
        </span>
      </div>
      {Object.entries(COMMANDS).map(([db, lines]) => (
        <pre
          key={db}
          data-option={db}
          hidden={db !== 'sqlite'}
          className="flex-1 overflow-x-auto bg-code px-5 py-5 font-mono text-code-fg text-mono-sm leading-loose"
        >
          <span className="text-fg-ghost">$ </span>
          <code>{lines.join('\n')}</code>
        </pre>
      ))}
    </div>
  );
}
