import Image from 'next/image';
import type { ReactNode } from 'react';
import { cn } from '@/lib/utils';
import { Reveal, Words } from '../home/reveal';

/**
 * The opening of the blog, the changelog and the roadmap: the landing's
 * section head at page size. Eyebrow, a title rising word by word, the lead,
 * and an optional line of meta under it.
 */
export function PageIntro({
  eyebrow,
  title,
  lead,
  meta,
}: {
  eyebrow: string;
  title: string;
  lead: ReactNode;
  meta?: ReactNode;
}) {
  return (
    // `CONTAINER` written out: a server component importing a constant from a
    // client module gets a reference to it, not the string.
    <header className="mx-auto flex w-full max-w-6xl flex-col gap-5 px-6 pt-36 sm:pt-44">
      <Reveal y={12}>
        <p className="font-mono text-fg-brand text-mono uppercase tracking-wider">
          {eyebrow}
        </p>
      </Reveal>
      <Words
        as="h1"
        text={title}
        delay={0.1}
        className="max-w-[16ch] text-balance text-display text-fg"
      />
      <Reveal delay={0.35}>
        <p className="max-w-xl text-pretty text-fg-subtle text-lead">{lead}</p>
      </Reveal>
      {meta ? (
        <Reveal
          delay={0.45}
          className="flex flex-wrap items-center gap-x-3 gap-y-1 text-fg-meta text-meta"
        >
          {meta}
        </Reveal>
      ) : null}
    </header>
  );
}

/**
 * The landing's product frame without the product: the blurred lime photo,
 * pulled towards the canvas, with whatever sits on it in front.
 */
export function Backdrop({
  children,
  className,
  dim = false,
}: {
  children: ReactNode;
  className?: string;
  /** Pulled further towards the canvas, for text set straight on it. */
  dim?: boolean;
}) {
  return (
    <div
      className={cn(
        'relative overflow-hidden rounded-3xl border border-border-subtle',
        className,
      )}
    >
      <Image
        fill
        src="/home/lime-swirl.jpg"
        alt=""
        sizes="(min-width: 72rem) 72rem, 100vw"
        className="scale-110 object-cover blur-2xl saturate-[0.85]"
      />
      <div
        className={cn(
          'absolute inset-0',
          dim ? 'bg-canvas/55' : 'bg-canvas/30',
        )}
      />
      <div className="relative">{children}</div>
    </div>
  );
}
