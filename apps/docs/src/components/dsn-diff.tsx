import { cn, Tab, TabList, TabPanel, Tabs } from '@rustrak/ui';
import { highlight } from './code';

/*
 * The DSN line, before and after, in five SDKs. Rendered once at build time
 * with every panel in the HTML (`keepMounted`); `tabs.ts` switches them, so
 * no React reaches the browser for five tabs.
 */

const OLD = 'https://abc123@o447951.ingest.sentry.io/1234567';
const NEW = 'https://abc123@errors.acme.dev/1';

const SDKS = [
  {
    id: 'javascript',
    label: 'JavaScript',
    before: ["import * as Sentry from '@sentry/node';", '', 'Sentry.init({'],
    dsn: (url: string) => `  dsn: "${url}",`,
    after: ['  tracesSampleRate: 1.0,', '});'],
  },
  {
    id: 'python',
    label: 'Python',
    before: ['import sentry_sdk', '', 'sentry_sdk.init('],
    dsn: (url: string) => `    dsn="${url}",`,
    after: ['    traces_sample_rate=1.0,', ')'],
  },
  {
    id: 'go',
    label: 'Go',
    before: [
      'import "github.com/getsentry/sentry-go"',
      '',
      'sentry.Init(sentry.ClientOptions{',
    ],
    dsn: (url: string) => `    Dsn: "${url}",`,
    after: ['    TracesSampleRate: 1.0,', '})'],
  },
  {
    id: 'rust',
    label: 'Rust',
    before: ['use sentry;', '', 'let _guard = sentry::init(('],
    dsn: (url: string) => `    "${url}",`,
    after: ['    sentry::ClientOptions::default(),', '));'],
  },
  {
    id: 'ruby',
    label: 'Ruby',
    before: ["require 'sentry-ruby'", '', 'Sentry.init do |config|'],
    dsn: (url: string) => `  config.dsn = '${url}'`,
    after: ['  config.traces_sample_rate = 1.0', 'end'],
  },
];

export function DsnDiff() {
  const line = (text: string, key: string, mark = ' ', tone = '') => (
    <div key={key} className={cn('whitespace-pre px-5', tone)}>
      <span className="me-4 inline-block w-3 select-none">{mark}</span>
      {text ? highlight(text) : ' '}
    </div>
  );
  return (
    <Tabs defaultValue="javascript" className="flex h-full flex-col" data-tabs>
      <TabList className="px-4">
        {SDKS.map((sdk) => (
          <Tab key={sdk.id} value={sdk.id}>
            {sdk.label}
          </Tab>
        ))}
      </TabList>
      {SDKS.map((sdk) => (
        <TabPanel
          key={sdk.id}
          value={sdk.id}
          keepMounted
          className="flex-1 bg-code"
        >
          <pre className="py-6 font-mono text-mono-sm text-code-fg leading-loose">
            {sdk.before.map((text, i) => line(text, `b${i}`))}
            {line(
              sdk.dsn(OLD),
              'old',
              '−',
              'bg-sev-error-surface line-through decoration-sev-error-fg/60',
            )}
            {line(sdk.dsn(NEW), 'new', '+', 'bg-success-surface')}
            {sdk.after.map((text, i) => line(text, `a${i}`))}
          </pre>
        </TabPanel>
      ))}
    </Tabs>
  );
}
