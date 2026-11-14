import type { Metadata } from 'next';
import Link from 'next/link';
import { notFound } from 'next/navigation';
import {
  Faq,
  Figure,
  faqJsonLd,
  Heading,
  ResultsTable,
  Stat,
} from '@/components/compare/figures';
import {
  CoreBudget,
  HowMany,
  Replay,
  ServerFit,
} from '@/components/compare/vs-visuals';
import { Backdrop, PageIntro } from '@/components/editorial/intro';
import { Shell } from '@/components/editorial/shell';
import { Reveal } from '@/components/home/reveal';
import { CONTAINER } from '@/components/home/site';
import { MEASURED, RUNS } from '@/lib/compare';
import { baseMetadata, jsonLd, SITE } from '@/lib/seo';
import {
  COMPETITORS,
  FACT_LABELS,
  type FactKey,
  RUSTRAK_FACTS,
  type Story,
} from '@/lib/vs';

type PageProps = { params: Promise<{ slug: string }> };

export const dynamicParams = false;

export function generateStaticParams() {
  return COMPETITORS.map((c) => ({ slug: c.slug }));
}

const find = (slug: string) => COMPETITORS.find((c) => c.slug === slug);

export async function generateMetadata({
  params,
}: PageProps): Promise<Metadata> {
  const c = find((await params).slug);
  if (!c) return {};
  const url = `/vs/${c.slug}`;
  return {
    title: { absolute: c.title },
    description: c.description,
    alternates: { canonical: url },
    openGraph: {
      ...baseMetadata.openGraph,
      title: c.title,
      description: c.description,
      url,
    },
    twitter: {
      ...baseMetadata.twitter,
      title: c.title,
      description: c.description,
    },
  };
}

const SECTION = `${CONTAINER} py-16 sm:py-24`;
const PILL =
  'rounded-pill border border-border-subtle bg-canvas/60 px-5 py-2.5 text-control text-fg transition-colors hover:border-border';

/** The page's own picture of where Rustrak pulls ahead. */
function Visual({ story, runs }: { story: Story; runs: string[] }) {
  switch (story.visual) {
    case 'fit':
      return <ServerFit ids={['rustrak', 'sentry-errors', 'sentry']} />;
    case 'howmany':
      return <HowMany other="sentry-errors" />;
    case 'replay':
      return <Replay ids={runs} speed={story.speed} />;
    case 'core':
      return <CoreBudget ids={runs} />;
  }
}

/** The whole move, as the reader would type it. */
function Switch() {
  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <Figure caption="Start Rustrak. One container answers with the dashboard and the API on port 8080.">
        <pre className="overflow-x-auto font-mono text-fg-secondary text-mono leading-relaxed">
          <span className="text-fg-meta"># one container, one volume</span>
          {`
docker run -d --name rustrak -p 8080:8080 \\
  -v rustrak_data:/data \\
  -e SESSION_SECRET_KEY=$(openssl rand -hex 32) \\
  -e CREATE_SUPERUSER=you@example.com:change-me \\
  -e PUBLIC_URL=https://errors.example.com \\
  rustrak/rustrak-server:latest`}
        </pre>
      </Figure>
      <Figure caption="Create a project, copy its DSN and swap it in. Nothing else in the SDK setup changes.">
        <pre className="overflow-x-auto font-mono text-mono leading-relaxed">
          <span className="text-fg-meta">{'Sentry.init({'}</span>
          {'\n'}
          <span className="text-fg-ghost line-through">
            {'  dsn: "https://key@old-host/1",'}
          </span>
          {'\n'}
          <span className="text-fg-brand">
            {'  dsn: "https://key@errors.example.com/1",'}
          </span>
          {'\n'}
          <span className="text-fg-meta">{'  tracesSampleRate: 0.2,'}</span>
          {'\n'}
          <span className="text-fg-meta">{'});'}</span>
        </pre>
      </Figure>
    </div>
  );
}

export default async function Page({ params }: PageProps) {
  const c = find((await params).slug);
  if (!c) notFound();

  const runs = RUNS.filter((r) => c.runs.includes(r.id));
  const others = COMPETITORS.filter((o) => o.slug !== c.slug);
  const keys = Object.keys(FACT_LABELS) as FactKey[];

  const structuredData = {
    '@context': 'https://schema.org',
    '@graph': [
      {
        '@type': 'TechArticle',
        headline: c.headline,
        description: c.description,
        url: `${SITE}/vs/${c.slug}`,
        datePublished: MEASURED,
        dateModified: MEASURED,
        author: { '@id': `${SITE}/#organization` },
        publisher: { '@id': `${SITE}/#organization` },
        about: [
          { '@type': 'SoftwareApplication', name: 'Rustrak', url: SITE },
          { '@type': 'SoftwareApplication', name: c.name },
        ],
      },
      faqJsonLd(c.faq),
    ],
  };

  return (
    <Shell>
      <script {...jsonLd(structuredData)} />
      <PageIntro
        eyebrow="Compare"
        title={c.headline}
        lead={c.lead}
        meta={
          <>
            <span>Measured {MEASURED}</span>
            <span aria-hidden>·</span>
            <Link
              href="/benchmarks#method"
              className="transition-colors hover:text-fg"
            >
              How we measured
            </Link>
          </>
        }
      />

      <section className={`${CONTAINER} pt-16 sm:pt-24`}>
        <div className="grid gap-4 md:grid-cols-3">
          {c.stats.map((s, i) => (
            <Stat
              key={s.label}
              value={s.value}
              label={s.label}
              delay={i * 0.06}
            />
          ))}
        </div>
      </section>

      {c.stories.map((story) => (
        <section key={story.title} className={SECTION}>
          <Heading title={story.title} lead={story.lead} />
          <div className="mt-10">
            <Figure caption={story.caption}>
              <Visual story={story} runs={c.runs} />
            </Figure>
          </div>
        </section>
      ))}

      <section className={SECTION}>
        <Heading
          id="numbers"
          title="Every number"
          lead="Same machine, same four cores, one product at a time. Every product stored every event."
        />
        <div className="mt-10">
          <ResultsTable runs={runs} />
        </div>
      </section>

      <section className={SECTION}>
        <Heading id="setup" title="What you would be running" />
        <Reveal
          y={40}
          className="mt-10 overflow-x-auto rounded-3xl border border-border-subtle"
        >
          <table className="w-full min-w-[36rem] border-collapse text-start">
            <thead>
              <tr className="border-border-subtle border-b">
                <th className="w-[30%] p-4 sm:p-5" />
                <th
                  scope="col"
                  className="bg-surface p-4 text-start text-fg-brand text-section sm:p-5"
                >
                  Rustrak
                </th>
                <th
                  scope="col"
                  className="p-4 text-start text-fg text-section sm:p-5"
                >
                  {c.name}
                </th>
              </tr>
            </thead>
            <tbody>
              {keys.map((key) => (
                <tr
                  key={key}
                  className="border-border-divider border-b last:border-b-0"
                >
                  <th
                    scope="row"
                    className="p-4 text-start font-normal text-fg-meta text-value sm:p-5"
                  >
                    {FACT_LABELS[key]}
                  </th>
                  <td className="bg-surface p-4 text-fg text-value sm:p-5">
                    {RUSTRAK_FACTS[key]}
                  </td>
                  <td className="p-4 text-fg-subtle text-value sm:p-5">
                    {c.facts[key]}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </Reveal>
      </section>

      <section className={SECTION}>
        <Heading
          id="switch"
          title="Switching is one line"
          lead="Your apps already speak the Sentry protocol. Start Rustrak, then point the DSN at it."
        />
        <div className="mt-10">
          <Switch />
        </div>
      </section>

      <section className={SECTION}>
        <Heading id="faq" title="Questions" />
        <Faq items={c.faq} />
      </section>

      <section className={SECTION}>
        <Reveal y={48}>
          <Backdrop dim>
            <div className="flex flex-col gap-8 px-6 py-16 sm:px-12 sm:py-20">
              <h2 className="max-w-xl text-balance text-fg text-headline">
                Run it on the machine you already have
              </h2>
              <div className="flex flex-wrap gap-3">
                <Link
                  href="/getting-started/installation"
                  className="rounded-pill bg-surface-brand px-5 py-2.5 text-control text-fg-on-brand"
                >
                  Install Rustrak
                </Link>
                <Link href="/benchmarks" className={PILL}>
                  The full benchmark
                </Link>
                {others.map((o) => (
                  <Link key={o.slug} href={`/vs/${o.slug}`} className={PILL}>
                    {o.headline}
                  </Link>
                ))}
              </div>
            </div>
          </Backdrop>
        </Reveal>
      </section>
    </Shell>
  );
}
