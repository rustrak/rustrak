/**
 * The design system's `cn`, which knows its token scale as well as Tailwind's
 * own. A plain tailwind-merge does not: it reads `text-sm` and `text-label` as
 * unrelated classes, keeps both, and the shadcn default wins over the token a
 * screen asked for.
 */
export { cn } from '@rustrak/ui';

/**
 * Format a byte count as a human-readable size (e.g. 1536 → "1.5 KB").
 * Uses binary units (1024). Returns "0 B" for zero/negative input.
 */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return '0 B';
  }
  const units = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];
  const exponent = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    units.length - 1,
  );
  const value = bytes / 1024 ** exponent;
  const formatted = exponent === 0 ? String(value) : value.toFixed(1);
  return `${formatted} ${units[exponent]}`;
}

/** The attachment formats the server serves inline, so an `<img>` can show. */
export function isInlineImage(contentType: string): boolean {
  return ['image/png', 'image/jpeg', 'image/gif', 'image/webp'].includes(
    contentType,
  );
}
