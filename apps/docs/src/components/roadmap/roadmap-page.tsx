import { Button } from '@rustrak/ui';
import { fetchRoadmap, PROJECT_URL } from '@/lib/roadmap';
import { Backdrop, PageIntro } from '../editorial/intro';
import { Shell } from '../editorial/shell';
import { Reveal, Words } from '../home/reveal';
import { RoadmapLive } from './live';

/* Written out rather than imported from `home/chrome`: this is a server
   module, and a constant from a client module arrives as a reference. */
const ISSUES = 'https://github.com/rustrak/rustrak/issues/new';

/**
 * The public roadmap: the intro, the whole plan as a board, each horizon in
 * full, and a way to ask for what is missing. All of it is the GitHub project
 * (`lib/roadmap.ts`): fetched at build so the page ships drawn, then again in
 * the browser so it shows the project as it is now.
 */
export async function RoadmapPage() {
  // A build that cannot reach GitHub still builds; the browser fetches anyway.
  const initial = await fetchRoadmap().catch(() => []);

  return (
    <Shell>
      <PageIntro
        eyebrow="Roadmap"
        title="What we are building next."
        lead="In order of priority, not on a calendar. What is in flight now, what comes after it, and what we are still thinking about."
        meta={
          <a href={PROJECT_URL} className="transition-colors hover:text-fg">
            Live from GitHub Projects ↗
          </a>
        }
      />

      <RoadmapLive initial={initial} />

      <section className="mx-auto w-full max-w-7xl px-4 py-24 sm:px-6 sm:py-32">
        <Reveal y={48}>
          <Backdrop dim>
            <div className="flex flex-col items-center px-6 py-24 text-center sm:py-28">
              <Words
                text="Missing something?"
                delay={0.3}
                className="text-balance text-fg text-headline"
              />
              <Reveal delay={0.55}>
                <p className="mt-5 max-w-md text-pretty text-fg-secondary text-lead">
                  The roadmap follows what people running Rustrak ask for. Tell
                  us what you need.
                </p>
              </Reveal>
              <Reveal delay={0.7} className="mt-8">
                <Button
                  variant="primary"
                  size="lg"
                  /* react-doctor-disable-next-line react-doctor/anchor-has-content */
                  render={<a href={ISSUES} />}
                >
                  Open an issue
                </Button>
              </Reveal>
            </div>
          </Backdrop>
        </Reveal>
      </section>
    </Shell>
  );
}
