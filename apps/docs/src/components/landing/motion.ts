/**
 * One easing curve for the entire landing. Motion reads
 * as designed rather than assembled when every element decelerates the same
 * way; mixing curves per section is what makes a page feel like a pile of
 * effects. Slow on purpose — the page is meant to feel spacious.
 */
export const EASE = [0.16, 1, 0.3, 1] as const;

/** Scroll-in trigger for the editorial pages: once, a little past the fold. */
export const VIEW = { once: true, margin: '-12% 0px' } as const;
