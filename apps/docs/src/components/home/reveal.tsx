'use client';

import * as m from 'motion/react-m';
import { Fragment, type ReactNode } from 'react';
import { EASE, VIEW } from '../landing/motion';

/**
 * The landing's two moves, both on the one curve: a block sharpens up out of
 * a blur, and a headline rises word by word out of a mask. Both fire once,
 * when they scroll in; above the fold that is the moment the page loads.
 */
export function Reveal({
  children,
  className,
  delay = 0,
  y = 24,
  as = 'div',
}: {
  children: ReactNode;
  className?: string;
  delay?: number;
  y?: number;
  as?: 'div' | 'li';
}) {
  const Tag = as === 'li' ? m.li : m.div;
  return (
    <Tag
      className={className}
      initial={{ opacity: 0, y, filter: 'blur(8px)' }}
      whileInView={{ opacity: 1, y: 0, filter: 'blur(0px)' }}
      viewport={VIEW}
      transition={{ duration: 1.1, ease: EASE, delay }}
    >
      {children}
    </Tag>
  );
}

export function Words({
  text,
  as = 'h2',
  className,
  delay = 0,
}: {
  text: string;
  as?: 'h1' | 'h2';
  className?: string;
  delay?: number;
}) {
  // The heading watches the viewport, not the words: a word sitting below its
  // mask is clipped out of view, so it would never see itself scroll in.
  const Tag = as === 'h1' ? m.h1 : m.h2;
  return (
    <Tag
      aria-label={text}
      className={className}
      initial="hidden"
      whileInView="shown"
      viewport={VIEW}
    >
      {text.split(' ').map((word, i) => (
        // Pieces of a fixed string: position is the identity.
        // react-doctor-disable-next-line react-doctor/no-array-index-as-key
        <Fragment key={`${i}-${word}`}>
          {i > 0 ? ' ' : null}
          <span
            aria-hidden
            // Clip only vertically, padded so descenders and tracking survive.
            className="-my-[0.25em] inline-block overflow-y-clip py-[0.25em] align-bottom"
          >
            <m.span
              className="inline-block"
              variants={{ hidden: { y: '150%' }, shown: { y: '0%' } }}
              transition={{
                duration: 1.2,
                ease: EASE,
                delay: delay + i * 0.06,
              }}
            >
              {word}
            </m.span>
          </span>
        </Fragment>
      ))}
    </Tag>
  );
}
