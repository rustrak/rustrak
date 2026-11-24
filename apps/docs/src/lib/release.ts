/**
 * What a release is, read off its file: pure functions, no Astro, so the
 * build and `release.test.ts` share them.
 */

export type ReleaseChannel = 'minor' | 'patch' | 'seed';

/** What a `##` block inside a release is about. Drives its label's tone. */
export type SectionKind = 'shipped' | 'improved' | 'fixed' | 'documented';

export function parseVersion(value: string): [number, number, number] | null {
  const match = /^v?(\d+)\.(\d+)\.(\d+)/.exec(value);
  return match ? [Number(match[1]), Number(match[2]), Number(match[3])] : null;
}

/** `v0.32.0` → `v0-32-0`: the anchor every link to a release already uses. */
export const anchorOf = (version: string) =>
  version.toLowerCase().replace(/[^a-z0-9]+/g, '-') || 'release';

export function channelOf(version: string): ReleaseChannel {
  const parsed = parseVersion(version);
  if (!parsed) return 'seed';
  return parsed[2] === 0 ? 'minor' : 'patch';
}

/**
 * Newest first, by version rather than by date: a patch backported after a
 * later minor carries the later date. `v0.1.x` and friends predate the
 * release line and can never be newest. Date and id only break ties between
 * equal versions (two release candidates of one version).
 */
export function byNewestFirst(
  a: { id: string; version: string; date: Date },
  b: { id: string; version: string; date: Date },
): number {
  const left = parseVersion(a.version);
  const right = parseVersion(b.version);
  if (left && right) {
    for (let i = 0; i < 3; i += 1) {
      if (left[i] !== right[i]) return right[i] - left[i];
    }
  } else if (left || right) {
    return left ? -1 : 1;
  }
  return b.date.valueOf() - a.date.valueOf() || b.id.localeCompare(a.id);
}

/**
 * Section headings are free text, so this reads intent rather than matching a
 * fixed vocabulary. Anything it does not recognise is new work, which is the
 * right default: those are the sections an entry writes a real title for.
 */
export function kindOf(title: string): SectionKind {
  const text = title.toLowerCase();
  if (/fix|bug|regression/.test(text)) return 'fixed';
  if (/improv|polish|enhance|update|tweak/.test(text)) return 'improved';
  if (/doc|guide|readme/.test(text)) return 'documented';
  return 'shipped';
}

export type Section = { title: string; kind: SectionKind; markdown: string };

/**
 * A release body, split on its `##` headings: each block gets its own label
 * and the entry reads as a short document rather than one run of prose.
 * Anything above the first heading is an untitled lead section, so a file
 * that opens with a paragraph is not dropped. A `##` inside a code fence is a
 * comment in TOML or shell, not a heading.
 */
export function splitSections(body: string): Section[] {
  const sections: { title: string; lines: string[] }[] = [
    { title: '', lines: [] },
  ];
  let fenced = false;
  for (const line of body.split('\n')) {
    if (/^\s*(```|~~~)/.test(line)) fenced = !fenced;
    const heading = fenced ? null : /^##\s+(.*)$/.exec(line);
    if (heading) sections.push({ title: heading[1].trim(), lines: [] });
    else sections[sections.length - 1].lines.push(line);
  }
  return sections
    .map(({ title, lines }) => ({
      title,
      kind: kindOf(title),
      markdown: lines.join('\n').trim(),
    }))
    .filter((section) => section.title !== '' || section.markdown !== '');
}

/**
 * How many changes a section describes: its list items, nested ones too (a
 * sub-item still shipped), or its paragraphs when it is written as prose.
 * A magnitude for the history strip, not an inventory.
 */
export function countChanges(html: string): number {
  const items = html.match(/<li[\s>]/g)?.length ?? 0;
  if (items > 0) return items;
  return html.match(/<p[\s>]/g)?.length ?? 0;
}
