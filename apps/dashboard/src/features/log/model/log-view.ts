import type { LogsVolumeResponse } from '@rustrak/client';

/** The six levels, least severe first. */
export const LOG_LEVELS = [
  'trace',
  'debug',
  'info',
  'warn',
  'error',
  'fatal',
] as const;

export type LogLevel = (typeof LOG_LEVELS)[number];

/**
 * The volume as bars of four bands. `fatal` folds into error and `trace`
 * into debug, as the overview's chart folds them: as bands of their own they
 * sit too close to their neighbour's colour to be told apart.
 */
export function volumeBars(points: LogsVolumeResponse) {
  return points.map((p) => ({
    t: Date.parse(p.bucket),
    debug: p.trace + p.debug,
    info: p.info,
    warn: p.warn,
    error: p.error + p.fatal,
  }));
}

/** `text` cut where it holds `query`, ignoring case, the matches marked. */
export function matchParts(
  text: string,
  query: string,
): { text: string; match: boolean }[] {
  const needle = query.trim();
  if (!needle) return [{ text, match: false }];
  // Matched on the text itself, not a lowercased copy: some letters lowercase
  // longer (`İ`), and positions read off the copy would shift.
  const pattern = new RegExp(
    needle.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'),
    'giu',
  );
  const parts: { text: string; match: boolean }[] = [];
  let from = 0;
  for (const found of text.matchAll(pattern)) {
    const at = found.index;
    if (at > from) parts.push({ text: text.slice(from, at), match: false });
    parts.push({ text: found[0], match: true });
    from = at + found[0].length;
  }
  if (from < text.length) parts.push({ text: text.slice(from), match: false });
  return parts;
}

export interface AttributeGroup {
  /** What the keys start with before their first dot; '' for bare keys. */
  name: string;
  attributes: { key: string; value: string }[];
}

/**
 * A log's attributes as Sentry's attribute tree shows them: grouped by
 * namespace (`sentry.`, `user.`...), bare keys first, each value as text.
 * Logs carry the OTel-style typed map (`{ value, type }`); anything else is
 * shown as JSON.
 */
export function attributeGroups(attributes: unknown): AttributeGroup[] {
  const groups = new Map<string, AttributeGroup['attributes']>();
  const entries = Object.entries((attributes ?? {}) as Record<string, unknown>);
  for (const [key, raw] of entries.sort(([a], [b]) => a.localeCompare(b))) {
    const dot = key.indexOf('.');
    const name = dot > 0 ? key.slice(0, dot) : '';
    const group = groups.get(name) ?? [];
    group.push({ key, value: attributeText(raw) });
    groups.set(name, group);
  }
  return [...groups]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([name, attributes]) => ({ name, attributes }));
}

function attributeText(raw: unknown): string {
  const value =
    raw !== null && typeof raw === 'object' && 'value' in raw
      ? (raw as { value: unknown }).value
      : raw;
  return typeof value === 'string' ? value : JSON.stringify(value);
}
