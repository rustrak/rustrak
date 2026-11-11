'use client';

import { InteractiveDemo as OpenShowcaseDemo } from '@rustrak/openshowcase-player-react';

/**
 * An OpenShowcase demo exported to `public/demos/`. The player fetches the
 * file, so `src` is a path under `public/`. The site is dark only, so the
 * player is too.
 */
export function InteractiveDemo({ src }: { src: string }) {
  return (
    <OpenShowcaseDemo
      src={src}
      hotspotStyle={{
        bgColor: 'var(--surface-brand)',
      }}
      appearance="dark"
      wrapper="bezel"
      backdrop={{ color: 'var(--surface-brand)', fill: 'soft', padding: 2 }}
      startWhenVisible
      // The player draws its own window frame; a border here doubles it.
      className="my-6 w-full"
    />
  );
}
