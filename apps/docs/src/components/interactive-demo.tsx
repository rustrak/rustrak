'use client';

import { InteractiveDemo as OpenShowcaseDemo } from '@rustrak/openshowcase-player-react';
import { useEffect, useState } from 'react';

/**
 * An OpenShowcase demo exported to `public/demos/`. The player fetches the
 * file, so `src` is a path under `public/`, prefixed with the base path the
 * site is served from, the same way `<Video>` does it.
 */
export function InteractiveDemo({ src }: { src: string }) {
  const [appearance, setAppearance] = useState<'light' | 'dark'>('light');

  // Nextra's theme switch toggles the `dark` class on <html>.
  useEffect(() => {
    const root = document.documentElement;
    const update = () =>
      setAppearance(root.classList.contains('dark') ? 'dark' : 'light');
    update();
    const observer = new MutationObserver(update);
    observer.observe(root, { attributes: true, attributeFilter: ['class'] });
    return () => observer.disconnect();
  }, []);

  const basePath = process.env.NEXT_PUBLIC_BASE_PATH || '';

  return (
    <OpenShowcaseDemo
      src={src.startsWith('/') ? `${basePath}${src}` : src}
      appearance={appearance}
      // The player draws its own window frame; a border here doubles it.
      className="my-6 w-full"
    />
  );
}
