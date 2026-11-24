import { getCollection } from 'astro:content';
import { Marked } from 'marked';
import {
  anchorOf,
  byNewestFirst,
  channelOf,
  countChanges,
  type ReleaseChannel,
  type SectionKind,
  splitSections,
} from './lib/release';

/**
 * The changelog as data: every release, newest version first, its body split
 * into labelled sections and rendered to HTML.
 *
 * The sections are rendered with `marked`, not the site's Markdown pipeline,
 * because the page needs each `##` block on its own (with its own label and
 * count), and `render()` only hands back a whole entry.
 */

export type Release = {
  /** The filename, kept as a second anchor so old deep links still land. */
  id: string;
  /** `v0-32-0`: the page of the release, and its anchor on the index. */
  anchor: string;
  version: string;
  channel: ReleaseChannel;
  title: string;
  description: string;
  date: Date;
  tags: string[];
  sections: { title: string; kind: SectionKind; html: string }[];
  /** Changes in the release, for the history strip. Never below one. */
  changes: number;
};

/** A private instance: options on the `marked` singleton are process-wide. */
const md = new Marked({ gfm: true });

let cache: Release[] | undefined;

export async function getReleases(): Promise<Release[]> {
  cache ??= (await getCollection('changelog'))
    .map(({ id, body, data }): Release => {
      const sections = splitSections(body ?? '').map((section) => ({
        title: section.title,
        kind: section.kind,
        html: md.parse(section.markdown) as string,
      }));
      return {
        id,
        anchor: anchorOf(data.version),
        version: data.version,
        channel: channelOf(data.version),
        title: data.title,
        description: data.description,
        date: data.date,
        tags: data.tags,
        sections,
        changes: Math.max(
          1,
          sections.reduce((sum, s) => sum + countChanges(s.html), 0),
        ),
      };
    })
    .sort(byNewestFirst);
  return cache;
}

/** How many releases the index shows in full; the rest are one line each. */
export const ON_INDEX = 10;
