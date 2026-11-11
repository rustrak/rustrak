import fs from 'node:fs';
import path from 'node:path';
import matter from 'gray-matter';
import { Marked } from 'marked';
import {
  parseVersion,
  type Release,
  type ReleaseChannel,
  type ReleaseChunk,
  type ReleasePulse,
  type ReleaseSection,
  type SectionKind,
  safeDate,
} from './release.ts';

/**
 * The changelog, read off `content/changelog/*.mdx` and turned into data.
 *
 * Server-only: it opens the filesystem. Anything the browser also needs lives
 * in `./release`.
 *
 * ── Why this stopped going through Nextra ──────────────────────────────────
 *
 * The page used to render each release with `importPage()`, which compiles the
 * file as an MDX *page* and hands back a React component. That works, and it
 * has one property this page cannot live with: a component can only be
 * rendered by the server that imported it, so every release had to be in the
 * first response. Forty-one of them came to 604KB of HTML and a 296KB RSC
 * payload for a single URL, growing by roughly 20KB per release.
 *
 * Rendering to an HTML string instead makes a release *transferable*: the same
 * shape can be inlined into the page or fetched later from a static file (see
 * `app/(site)/changelog/releases/[chunk]/route.ts`). The content affords it —
 * every entry is plain Markdown, with no JSX, no imports and no code fences —
 * so nothing is lost by leaving the MDX pipeline out of it.
 */

/**
 * How many releases the page ships with, and how many each later fetch brings.
 * Ten covers "what changed recently" for almost every visitor without a second
 * request, and caps the first response at a fraction of what it used to be.
 */
export const RELEASES_PER_CHUNK = 10;

const CHANGELOG_DIR = path.join(process.cwd(), 'content/changelog');

/**
 * A private instance rather than the shared `marked` singleton: options set on
 * the singleton are global to the process, and this module is not the only
 * thing that could be rendering Markdown during a build.
 */
const md = new Marked({ gfm: true, breaks: false });

function toDateString(value: unknown): string {
  if (value instanceof Date) return value.toISOString().split('T')[0];
  if (typeof value === 'string' && value) return value;
  return new Date().toISOString().split('T')[0];
}

/**
 * Newest first, by version rather than by date; `versions.json`
 * (`scripts/generate-versions.mjs`) is built from this same list. A patch backported after a
 * later minor carries the later date, and the numeric filename prefixes stop
 * sorting lexicographically at three digits, so neither is safe on its own.
 * Date and slug only break ties between equal versions.
 */
function byNewestFirst(a: Release, b: Release): number {
  const left = parseVersion(a.version);
  const right = parseVersion(b.version);

  if (left && right) {
    for (let i = 0; i < 3; i += 1) {
      if (left[i] !== right[i]) return right[i] - left[i];
    }
  } else if (left || right) {
    // `v0.1.x` and friends predate the release line and can never be newest.
    return left ? -1 : 1;
  }

  const byDate =
    (safeDate(b.date)?.getTime() ?? 0) - (safeDate(a.date)?.getTime() ?? 0);
  if (byDate !== 0) return byDate;
  return b.slug.localeCompare(a.slug);
}

function channelOf(version: string): ReleaseChannel {
  const parsed = parseVersion(version);
  if (!parsed) return 'seed';
  return parsed[2] === 0 ? 'minor' : 'patch';
}

/**
 * Section headings are free text, so this reads intent rather than matching a
 * fixed vocabulary. Anything it does not recognise is treated as new work,
 * which is the right default: those are the sections an entry writes a real
 * title for.
 */
function kindOf(title: string): SectionKind {
  const text = title.toLowerCase();
  if (/fix|bug|regression/.test(text)) return 'fixed';
  if (/improv|polish|enhance|update|tweak/.test(text)) return 'improved';
  if (/doc|guide|readme/.test(text)) return 'documented';
  return 'shipped';
}

/**
 * Splits a release body on its `##` headings.
 *
 * The split is what gives the page a hierarchy to render: without it a release
 * is one undifferentiated block of prose, and the headings inside it compete
 * with the release title for the same visual weight. With it, each block gets
 * its own numbered rule and the entry reads as a short document.
 *
 * Anything above the first heading becomes an untitled lead section, so a file
 * that opens with a paragraph is not silently dropped.
 */
function splitSections(body: string): ReleaseSection[] {
  const sections: { title: string; lines: string[] }[] = [];
  let current: { title: string; lines: string[] } = { title: '', lines: [] };

  for (const line of body.split('\n')) {
    const heading = /^##\s+(.*)$/.exec(line);
    if (heading) {
      sections.push(current);
      current = { title: heading[1].trim(), lines: [] };
    } else {
      current.lines.push(line);
    }
  }
  sections.push(current);

  // One release's sections, a handful of them, parsed once at build.
  // react-doctor-disable-next-line react-doctor/js-combine-iterations
  return sections
    .map((section) => ({
      title: section.title,
      kind: kindOf(section.title),
      html: md.parse(section.lines.join('\n').trim()) as string,
    }))
    .filter((section) => section.title !== '' || section.html !== '');
}

function readRelease(filename: string): Release {
  const raw = fs.readFileSync(path.join(CHANGELOG_DIR, filename), 'utf-8');
  const { data, content } = matter(raw);
  const version = String(data.version ?? '');

  return {
    slug: filename.replace(/\.mdx$/, ''),
    anchor: version.toLowerCase().replace(/[^a-z0-9]+/g, '-') || 'release',
    version,
    channel: channelOf(version),
    title: data.title ?? 'Untitled',
    description: data.description ?? '',
    date: toDateString(data.date),
    tags: Array.isArray(data.tags) ? data.tags.map(String) : [],
    sections: splitSections(content.trim()),
  };
}

/**
 * Parsed once per process. The page and every chunk of the static route read
 * the same list during a single build, and re-reading every file and
 * re-rendering their Markdown for each of them is pure waste.
 */
let cache: Release[] | null = null;

export function getReleases(): Release[] {
  if (cache) return cache;

  // react-doctor-disable-next-line react-doctor/js-combine-iterations
  cache = fs
    .readdirSync(CHANGELOG_DIR)
    .filter((filename) => filename.endsWith('.mdx'))
    .map(readRelease)
    .sort(byNewestFirst);

  return cache;
}

/**
 * One-based. Chunk 1 is inlined into the page and the rest are fetched from the
 * static route, which is why both share this indexing.
 */
export function getReleaseChunk(chunk: number): ReleaseChunk {
  const all = getReleases();
  const start = (chunk - 1) * RELEASES_PER_CHUNK;
  const releases = all.slice(start, start + RELEASES_PER_CHUNK);
  return { releases, hasMore: start + releases.length < all.length };
}

export function getChunkCount(): number {
  return Math.max(1, Math.ceil(getReleases().length / RELEASES_PER_CHUNK));
}

/**
 * How many discrete changes a section describes.
 *
 * Counting list items is the honest measure for this content: every entry in
 * `content/changelog/` is written as a heading and a list of the things that
 * landed under it, so one `<li>` is one change. The paragraph fallback covers
 * the handful of sections written as prose: v0.10.2 is two sentences and no
 * list, and scoring it zero would drop a real release out of the spectrum
 * entirely.
 *
 * Nested items are counted too, and that is deliberate rather than overlooked:
 * a sub-item is still something that shipped, and excluding it would need the
 * HTML parsed into a tree to find out which items are nested. This is a
 * magnitude, not an inventory.
 */
function countChanges(html: string): number {
  const items = html.match(/<li[\s>]/g)?.length ?? 0;
  if (items > 0) return items;
  return html.match(/<p[\s>]/g)?.length ?? 0;
}

function pulseOf(release: Release): ReleasePulse {
  const counts: Record<SectionKind, number> = {
    shipped: 0,
    improved: 0,
    fixed: 0,
    documented: 0,
  };

  let total = 0;
  for (const section of release.sections) {
    const changes = countChanges(section.html);
    counts[section.kind] += changes;
    total += changes;
  }

  return {
    anchor: release.anchor,
    version: release.version,
    title: release.title,
    date: release.date,
    channel: release.channel,
    counts,
    // Floored at one so a release can never draw as nothing. An entry with
    // neither a list nor a paragraph is malformed, but the spectrum is a map of
    // the history and a missing column reads as a missing release.
    total: Math.max(1, total),
  };
}

/**
 * The whole history, at the resolution the spectrum draws it. Newest first,
 * matching `getReleases()`; the spectrum reverses it, because time reads left
 * to right and the archive reads top to bottom.
 */
export function getPulse(): ReleasePulse[] {
  return getReleases().map(pulseOf);
}
