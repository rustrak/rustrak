import { type CollectionEntry, getCollection } from 'astro:content';

export type DocPage = CollectionEntry<'docs'>;

/** `usage/alerts/index` → `usage/alerts`: a folder's index answers for it. */
export const slugOf = (page: DocPage) => page.id.replace(/\/index$/, '');

export const getDocs = () => getCollection('docs');

/** The file on GitHub, for "Edit this page". */
export const editUrl = (page: DocPage) =>
  `https://github.com/rustrak/rustrak/edit/main/apps/docs/content/${page.filePath?.split('/content/')[1] ?? `${page.id}.mdx`}`;

/** The page's own `# Title`, which its header sets instead. */
export const titleOf = (page: DocPage, fallback: string) =>
  page.data.title ??
  /^#[ \t]+(\S.*)$/m.exec(page.body ?? '')?.[1]?.trim() ??
  fallback;
