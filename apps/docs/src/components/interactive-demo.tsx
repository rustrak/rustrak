'use client';

import { InteractiveDemo as OpenShowcaseDemo } from '@rustrak/openshowcase-player-react';

/**
 * An OpenShowcase demo exported to `public/demos/`. The player fetches the
 * file, so `src` is a path under `public/`, prefixed with the base path the
 * site is served from. The site is dark only, so the player is too.
 */
export function InteractiveDemo({ src }: { src: string }) {
  const basePath = process.env.NEXT_PUBLIC_BASE_PATH || '';

  return (
    <OpenShowcaseDemo
      src={src.startsWith('/') ? `${basePath}${src}` : src}
      appearance="dark"
      startWhenVisible
      // The player draws its own window frame; a border here doubles it.
      className="my-6 w-full"
    />
  );
}
