'use client';

import { Button, StarIcon } from '@rustrak/ui';
import * as m from 'motion/react-m';
import Image from 'next/image';
import { EASE } from '../landing/motion';
import { AppFrame, OverviewScreen, Scaled } from './product';
import { Reveal, Words } from './reveal';
import { BACKDROP, CONTAINER, GITHUB, START } from './site';

/**
 * The claim, then the product on a landscape.
 *
 * The image is a backdrop, not a device: an abstract in the brand's lime,
 * blurred like every other product shot on the page. On it, the project overview floats at desktop size,
 * laid out at full width and scaled down, so it reads as a screenshot.
 */
export function Hero() {
  return (
    <section className="pt-36 sm:pt-44">
      <div className={`${CONTAINER} flex flex-col items-center text-center`}>
        <Words
          as="h1"
          text="Error tracking you actually own."
          delay={0.15}
          className="max-w-[14ch] text-balance text-display text-fg"
        />
        <Reveal delay={0.55}>
          <p className="mt-6 max-w-xl text-pretty text-lead text-fg-subtle">
            Point the Sentry SDKs you already use at one binary on your own
            server. Issues, traces, logs and alerts, with your data never
            leaving it.
          </p>
        </Reveal>
        <Reveal delay={0.7} className="mt-10 flex items-center gap-3">
          {/* react-doctor-disable-next-line react-doctor/anchor-has-content */}
          <Button variant="primary" size="lg" render={<a href={START} />}>
            Get started
          </Button>
          <Button
            variant="secondary"
            size="lg"
            icon={StarIcon}
            /* react-doctor-disable-next-line react-doctor/anchor-has-content */
            render={<a href={GITHUB} />}
          >
            Star on GitHub
          </Button>
        </Reveal>
      </div>

      {/* The product tips up out of the page, Linear-style, once the claim has landed. */}
      <div className="mx-auto mt-20 w-full max-w-7xl px-4 [perspective:2000px] sm:px-6">
        <m.div
          initial={{ opacity: 0, y: 80, rotateX: 18, scale: 0.94 }}
          animate={{ opacity: 1, y: 0, rotateX: 0, scale: 1 }}
          transition={{ duration: 1.6, ease: EASE, delay: 0.85 }}
          style={{ transformOrigin: '50% 0%' }}
          inert
          className="relative aspect-[16/10] overflow-hidden rounded-3xl border border-border-subtle"
        >
          <Image
            fill
            priority
            src={BACKDROP}
            alt=""
            sizes="(min-width: 80rem) 80rem, 100vw"
            className="scale-110 object-cover blur-2xl saturate-[0.85]"
          />
          <div className="absolute inset-0 bg-canvas/20" />

          <m.div
            initial={{ y: 60 }}
            animate={{ y: 0 }}
            transition={{ duration: 1.8, ease: EASE, delay: 1.05 }}
            className="absolute top-[7%] right-[5%] bottom-0 left-[5%] overflow-hidden rounded-t-2xl border-white/15 border-x border-t shadow-dialog"
          >
            <Scaled width={1440} className="size-full">
              <AppFrame section="overview">
                <OverviewScreen />
              </AppFrame>
            </Scaled>
          </m.div>
        </m.div>
      </div>
    </section>
  );
}
