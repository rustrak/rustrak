import { useTheme } from 'next-themes';
import { useEffect } from 'react';

/**
 * `--surface-panel` in each theme: the sheet the pages sit on, which on a phone
 * is the whole screen, so the status bar and the browser chrome continue it.
 * Hex because `theme-color` is read by the browser, not by CSS.
 */
const PANEL = { dark: '#222222', light: '#ffffff' } as const;

/**
 * Keeps `<meta name="theme-color">` on the theme the reader chose.
 *
 * `index.html` ships one tag per system scheme, which is right until somebody
 * picks a theme that differs from their system's: then the status bar would
 * stay the system's colour over a page in the other one. Both tags get the
 * chosen colour, so whichever the browser matches is correct. Renders nothing.
 */
export function ThemeColor() {
  const { resolvedTheme } = useTheme();

  useEffect(() => {
    const color = resolvedTheme === 'light' ? PANEL.light : PANEL.dark;
    for (const meta of document.querySelectorAll<HTMLMetaElement>(
      'meta[name="theme-color"]',
    )) {
      meta.content = color;
    }
  }, [resolvedTheme]);

  return null;
}
