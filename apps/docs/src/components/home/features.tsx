'use client';

import { ArrowRightIcon, cn, Tab, TabList, TabPanel, Tabs } from '@rustrak/ui';
import * as m from 'motion/react-m';
import Image from 'next/image';
import type { ReactNode } from 'react';
import { EASE, VIEW } from '../landing/motion';
import { SectionHead } from './chrome';
import { highlight } from './code';
import {
  AlertsScreen,
  AppFrame,
  IssueScreen,
  IssuesScreen,
  PerformanceScreen,
  Scaled,
} from './product';
import { Reveal } from './reveal';
import { BACKDROP, CONTAINER } from './site';

interface Point {
  title: string;
  body: string;
}

/**
 * One feature, in one of three compositions so a run of them does not read
 * as one block repeated: `wide` puts the product across the page with the
 * points in a row beneath it; `left` and `right` set it beside a column that
 * holds the heading and the points.
 */
function Feature({
  index,
  layout,
  eyebrow,
  title,
  lead,
  points,
  href,
  designWidth,
  interactive,
  children,
}: {
  index: number;
  layout: 'wide' | 'left' | 'right';
  designWidth?: number;
  interactive?: boolean;
  eyebrow: string;
  title: string;
  lead: string;
  points: Point[];
  href: string;
  children: ReactNode;
}) {
  const label = `${String(index).padStart(2, '0')} ${eyebrow}`;
  const docs = (
    <a
      href={href}
      className="group inline-flex items-center gap-1.5 self-start text-control text-fg transition-colors hover:text-fg-brand"
    >
      Read the docs
      <ArrowRightIcon className="size-3.5 transition-[translate] group-hover:translate-x-0.5" />
    </a>
  );

  if (layout === 'wide') {
    return (
      <section className="py-16 sm:py-20">
        <div className={CONTAINER}>
          <div className="grid items-end gap-6 lg:grid-cols-2 lg:gap-14">
            <SectionHead eyebrow={label} title={title} lead="" />
            <Reveal delay={0.35} className="flex flex-col gap-4 lg:pb-1">
              <p className="text-pretty text-fg-subtle text-lead">{lead}</p>
              {docs}
            </Reveal>
          </div>
          <Showcase designWidth={designWidth} className="mt-12 aspect-[16/8]">
            {children}
          </Showcase>
          <div className="mt-10 grid gap-8 sm:grid-cols-2 lg:grid-cols-4">
            {points.map((point, i) => (
              <Reveal
                key={point.title}
                delay={i * 0.08}
                className="flex flex-col gap-1.5 border-border-subtle border-t pt-5"
              >
                <h3 className="text-fg text-section">{point.title}</h3>
                <p className="text-fg-subtle text-value">{point.body}</p>
              </Reveal>
            ))}
          </div>
        </div>
      </section>
    );
  }

  return (
    <section className="py-16 sm:py-20">
      <div
        className={cn(
          CONTAINER,
          'grid items-center gap-12 lg:grid-cols-[minmax(0,3fr)_minmax(0,2fr)] lg:gap-16',
          layout === 'right' && 'lg:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]',
        )}
      >
        <Showcase
          designWidth={designWidth}
          interactive={interactive}
          className={cn('aspect-[5/4]', layout === 'right' && 'lg:order-2')}
        >
          {children}
        </Showcase>
        <div className="flex flex-col gap-8">
          <SectionHead eyebrow={label} title={title} lead={lead} />
          <div className="flex flex-col gap-5">
            {points.map((point, i) => (
              <Reveal
                key={point.title}
                delay={0.4 + i * 0.08}
                className="flex gap-3"
              >
                <span className="mt-2 size-1.5 shrink-0 rounded-pill bg-surface-brand" />
                <p className="text-fg-subtle text-value">
                  <span className="text-fg">{point.title}.</span> {point.body}
                </p>
              </Reveal>
            ))}
          </div>
          <Reveal delay={0.7}>{docs}</Reveal>
        </div>
      </div>
    </section>
  );
}

/**
 * The frame every product shot sits in: an abstract photo, blurred and pulled
 * towards the canvas, with the UI set into it from the top left and running
 * off the right and bottom edges.
 */
export function Showcase({
  designWidth = 1040,
  interactive = false,
  children,
  className,
}: {
  /** Leave the UI usable (the install command's tabs and copy button). */
  interactive?: boolean;
  /** The width the UI is laid out at before it is scaled into the frame. */
  designWidth?: number;
  children: ReactNode;
  className?: string;
}) {
  return (
    <m.div
      initial={{ opacity: 0, y: 48, scale: 0.96 }}
      whileInView={{ opacity: 1, y: 0, scale: 1 }}
      viewport={VIEW}
      transition={{ duration: 1.3, ease: EASE }}
      className={cn(
        'relative aspect-[16/11] overflow-hidden rounded-3xl border border-border-subtle bg-panel',
        className,
      )}
    >
      <Image
        fill
        sizes="100vw"
        src={BACKDROP}
        alt=""
        className="scale-110 object-cover blur-2xl saturate-[0.85]"
      />
      <div className="absolute inset-0 bg-canvas/20" />
      <m.div
        inert={!interactive}
        initial={{ opacity: 0, x: 48, y: 48 }}
        whileInView={{ opacity: 1, x: 0, y: 0 }}
        viewport={VIEW}
        transition={{ duration: 1.5, ease: EASE, delay: 0.25 }}
        className="absolute top-[9%] right-0 bottom-0 left-[7%] overflow-hidden rounded-tl-2xl border-white/15 border-t border-l bg-panel shadow-dialog"
      >
        <Scaled width={designWidth} cropped className="size-full">
          {children}
        </Scaled>
      </m.div>
    </m.div>
  );
}

export function Features() {
  return (
    <>
      <Feature
        index={1}
        layout="wide"
        eyebrow="Issues"
        title="Every error, grouped and ranked."
        lead="Thousands of events become a short list of issues, with how often each one happens, who it reaches and whether it is new."
        href="/usage/issues"
        points={[
          {
            title: 'Grouped by fingerprint',
            body: 'Events with the same cause land on one issue instead of a thousand rows.',
          },
          {
            title: 'New, regressed, escalating',
            body: 'What changed since you last looked is flagged on the row.',
          },
          {
            title: 'Trends at a glance',
            body: 'A 24-hour sparkline on every issue, without opening anything.',
          },
          {
            title: 'Assign and resolve',
            body: 'Hand an issue to a teammate, or resolve it in the next release.',
          },
        ]}
      >
        <AppFrame section="issues">
          <IssuesScreen />
        </AppFrame>
      </Feature>

      <Feature
        index={2}
        layout="left"
        eyebrow="Debug"
        title="The line that broke, and why."
        lead="Stack traces open on your own code, with the source around the failing line, the breadcrumbs before it and the release it shipped in."
        designWidth={1280}
        href="/usage/issues"
        points={[
          {
            title: 'Your frames first',
            body: 'Application code is expanded; library frames step back.',
          },
          {
            title: 'Source maps',
            body: 'Upload them and minified JavaScript reads as the code you wrote.',
          },
          {
            title: 'Breadcrumbs and context',
            body: 'The clicks, requests and logs that led up to the error.',
          },
          {
            title: 'Release health',
            body: 'Crash-free sessions per release, so a bad deploy shows up fast.',
          },
        ]}
      >
        <AppFrame
          section="issues"
          trail={[{ label: 'CHECKOUT-API-4F', mono: true }]}
        >
          <IssueScreen at="stack" rail="folded" />
        </AppFrame>
      </Feature>

      <Feature
        index={3}
        layout="right"
        eyebrow="Performance"
        title="See where the time goes."
        lead="Transactions and spans from the same SDKs: latency over time for every endpoint, and the waterfall of one slow request down to the query that held it up."
        designWidth={1280}
        href="/getting-started/overview"
        points={[
          {
            title: 'Ranked by time spent',
            body: 'Every endpoint by its share of total time, with p95 and failure rate beside it.',
          },
          {
            title: 'Span waterfall',
            body: 'Every database call, request and render inside one trace.',
          },
          {
            title: 'Any window',
            body: 'The last day, two weeks or all time, per environment.',
          },
          {
            title: 'Linked to errors',
            body: 'An error inside a trace opens on the request it broke.',
          },
        ]}
      >
        <AppFrame section="performance">
          <PerformanceScreen />
        </AppFrame>
      </Feature>

      <Feature
        index={4}
        layout="wide"
        eyebrow="Alerts"
        title="Only what is worth a message."
        lead="Alert rules decide what fires, check it before firing and route it to the people who can fix it. Nobody acknowledges it, it escalates."
        designWidth={1280}
        href="/usage/alerts"
        points={[
          {
            title: 'Conditions',
            body: 'New issues, regressions, or a level and a rate you choose.',
          },
          {
            title: 'Channels',
            body: 'Slack, email and webhooks, one rule to as many as you need.',
          },
          {
            title: 'Digests',
            body: 'A daily summary for what does not need anyone right now.',
          },
          {
            title: 'Escalation',
            body: 'Unacknowledged alerts move on to the next person on their own.',
          },
        ]}
      >
        <AppFrame section="alerts">
          <AlertsScreen />
        </AppFrame>
      </Feature>

      <Feature
        index={5}
        layout="left"
        eyebrow="Migrate"
        title="Keep your SDK. Change one line."
        lead="Rustrak speaks the Sentry protocol, so the official SDKs you already ship talk to it as they are. Point the DSN at your server; point it back to roll back."
        designWidth={600}
        interactive
        href="/getting-started/quickstart"
        points={[
          {
            title: 'Official SDKs',
            body: 'JavaScript, Python, Go, Rust, Ruby, PHP, Java and more.',
          },
          {
            title: 'No code changes',
            body: 'The DSN is the only line that moves.',
          },
          {
            title: 'Reversible',
            body: 'Switching back is the same one-line change.',
          },
          {
            title: 'Your data stays',
            body: 'Events never leave the server you run.',
          },
        ]}
      >
        <DsnDiff />
      </Feature>
    </>
  );
}

/* ------------------------------------------------------------------------ */

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

function DsnDiff() {
  const line = (text: string, key: string, mark = ' ', tone = '') => (
    <div key={key} className={cn('whitespace-pre px-5', tone)}>
      <span className="me-4 inline-block w-3 select-none">{mark}</span>
      {text ? highlight(text) : ' '}
    </div>
  );
  return (
    <Tabs defaultValue="javascript" className="flex h-full flex-col">
      <TabList className="px-4">
        {SDKS.map((sdk) => (
          <Tab key={sdk.id} value={sdk.id}>
            {sdk.label}
          </Tab>
        ))}
      </TabList>
      {SDKS.map((sdk) => (
        <TabPanel key={sdk.id} value={sdk.id} className="flex-1 bg-code">
          <pre className="py-6 font-mono text-mono-sm leading-loose text-code-fg">
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
