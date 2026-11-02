import { Button, StarIcon } from '@rustrak/ui';
import { MotionConfig } from 'motion/react';
import Image from 'next/image';
import { SmoothScroll } from '../landing/smooth-scroll';
import { CONTAINER, Footer, GITHUB, Nav, START } from './chrome';
import { Compare } from './compare';
import { Deploy } from './deploy';
import { BACKDROP, Features } from './features';
import { Flow } from './flow';
import { Hero } from './hero';
import { Reveal, Words } from './reveal';

/** The SDKs that already talk to Rustrak, named rather than drawn as logos. */
const SDKS = [
  'JavaScript',
  'Python',
  'Go',
  'Rust',
  'Ruby',
  'PHP',
  'Java',
  '.NET',
];

/**
 * The landing: the product on a real desk, four features shown on abstract
 * photographs, how to run it, and the way in. Everything enters once, as it
 * scrolls in (`reveal.tsx`); reduced motion keeps the fades and drops the moves.
 */
export function Home() {
  return (
    <MotionConfig reducedMotion="user">
      <SmoothScroll>
        <div className="bg-canvas text-fg">
          <Nav />
          <main>
            <Hero />
            <section className="py-16">
              <div
                className={`${CONTAINER} flex flex-col items-center gap-6 text-center`}
              >
                <Reveal y={12}>
                  <p className="text-meta text-fg-meta">
                    Works with the official Sentry SDKs for
                  </p>
                </Reveal>
                <ul className="flex flex-wrap justify-center gap-x-10 gap-y-3">
                  {SDKS.map((sdk, i) => (
                    <Reveal
                      as="li"
                      key={sdk}
                      y={12}
                      delay={0.1 + i * 0.05}
                      className="text-section text-fg-subtle"
                    >
                      {sdk}
                    </Reveal>
                  ))}
                </ul>
              </div>
            </section>
            <Flow />
            <Features />
            <Compare />
            <Deploy />
            <Closing />
          </main>
          <Footer />
        </div>
      </SmoothScroll>
    </MotionConfig>
  );
}

function Closing() {
  return (
    <section className="py-24 sm:py-32">
      <div className="mx-auto w-full max-w-7xl px-4 sm:px-6">
        <Reveal
          y={48}
          className="relative overflow-hidden rounded-3xl border border-border-subtle"
        >
          <Image
            fill
            src={BACKDROP}
            alt=""
            className="scale-110 object-cover blur-2xl saturate-[0.85]"
          />
          <div className="absolute inset-0 bg-canvas/55" />
          <div className="relative flex flex-col items-center px-6 py-28 text-center sm:py-36">
            <Words
              text="Your errors, on your server."
              delay={0.3}
              className="max-w-[16ch] text-balance text-display text-fg"
            />
            <Reveal delay={0.6}>
              <p className="mt-6 max-w-lg text-pretty text-lead text-fg-secondary">
                Open source under AGPL-3.0. No per-event pricing, no sampling
                you did not ask for.
              </p>
            </Reveal>
            <Reveal delay={0.75} className="mt-10 flex items-center gap-3">
              <Button variant="primary" size="lg" render={<a href={START} />}>
                Get started
              </Button>
              <Button
                variant="secondary"
                size="lg"
                icon={StarIcon}
                render={<a href={GITHUB} />}
              >
                Star on GitHub
              </Button>
            </Reveal>
          </div>
        </Reveal>
      </div>
    </section>
  );
}
