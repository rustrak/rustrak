import fs from 'node:fs';
import path from 'node:path';
import matter from 'gray-matter';
import readingTime from 'reading-time';

export type Post = {
  slug: string;
  title: string;
  description: string;
  date: string;
  author: string;
  tags: string[];
  image?: string;
  readingTime: string;
  draft: boolean;
  /**
   * The post's level-two headings, in order.
   *
   * This is what the index is built on. A blog of long technical write-ups is
   * badly served by a two-line standfirst, because the thing a reader is
   * deciding is whether the piece covers what they came for, and the section
   * titles answer that far better than a summary of them does. It is also the
   * only material this blog has to fill a wide frame with: there are no cover
   * images and there never will be, so the structure of the writing is the
   * illustration.
   */
  outline: string[];
};

const POSTS_DIR = path.join(process.cwd(), 'content/blog');

function ensurePostsDir() {
  if (!fs.existsSync(POSTS_DIR)) fs.mkdirSync(POSTS_DIR, { recursive: true });
}

function safeDate(s: string): Date | null {
  const d = new Date(s);
  return Number.isNaN(d.getTime()) ? null : d;
}

/**
 * The `##` headings, skipping anything inside a fenced block.
 *
 * The fence tracking is not defensive padding. These posts are about writing
 * Rust and configuring servers, so they are full of shell and TOML, and `##` is
 * a comment in both. Without it the source-maps post would list a comment out
 * of a config sample as one of its sections.
 */
function readOutline(body: string): string[] {
  const headings: string[] = [];
  let fenced = false;

  for (const line of body.split('\n')) {
    if (/^\s*(```|~~~)/.test(line)) {
      fenced = !fenced;
      continue;
    }
    if (fenced) continue;
    const match = /^##\s+(.+?)\s*$/.exec(line);
    // `###` and deeper are excluded by the `\s` after the second hash, which
    // is what keeps this an outline rather than a transcript of every heading.
    if (match) headings.push(match[1]);
  }

  return headings;
}

function readPost(filename: string): Post {
  const raw = fs.readFileSync(path.join(POSTS_DIR, filename), 'utf-8');
  const { data, content } = matter(raw);

  return {
    slug: filename.replace(/\.mdx$/, ''),
    title: data.title ?? 'Untitled',
    description: data.description ?? '',
    date:
      data.date instanceof Date
        ? data.date.toISOString().split('T')[0]
        : String(data.date ?? new Date().toISOString().split('T')[0]),
    author: data.author ?? 'Rustrak Team',
    tags: Array.isArray(data.tags) ? data.tags : [],
    image: data.image,
    readingTime: readingTime(content).text,
    draft: data.draft ?? false,
    outline: readOutline(content),
  };
}

export function getPosts(): Post[] {
  ensurePostsDir();
  // Runs once at build over a directory of a few files. Fusing the filter
  // into the map saves one small array and costs the reader the shape of
  // what this does.
  // react-doctor-disable-next-line react-doctor/js-combine-iterations
  return fs
    .readdirSync(POSTS_DIR)
    .filter((f) => f.endsWith('.mdx'))
    .map(readPost)
    .filter((p) => process.env.NODE_ENV === 'development' || !p.draft)
    .sort(
      (a, b) =>
        (safeDate(b.date)?.getTime() ?? 0) - (safeDate(a.date)?.getTime() ?? 0),
    );
}

export function getPost(slug: string): { post: Post; content: string } | null {
  ensurePostsDir();
  const filePath = path.join(POSTS_DIR, `${slug}.mdx`);
  if (!fs.existsSync(filePath)) return null;
  const { content } = matter(fs.readFileSync(filePath, 'utf-8'));
  return { post: readPost(`${slug}.mdx`), content };
}

/**
 * The two posts either side of this one, named by time rather than by
 * position. `getPosts()` is newest first, so "previous" and "next" mean
 * opposite things depending on whether you are thinking about the array or
 * about the calendar — which is exactly the kind of label that ends up
 * pointing the wrong way at the foot of a post.
 */
export function getNeighbourPosts(slug: string): {
  older: Post | null;
  newer: Post | null;
} {
  const posts = getPosts();
  const index = posts.findIndex((post) => post.slug === slug);
  if (index === -1) return { older: null, newer: null };
  return {
    older: posts[index + 1] ?? null,
    newer: posts[index - 1] ?? null,
  };
}

export function formatDate(dateStr: string): string {
  const d = safeDate(dateStr);
  if (!d) return dateStr;
  return d.toLocaleDateString('en-US', {
    year: 'numeric',
    month: 'long',
    day: 'numeric',
    timeZone: 'UTC',
  });
}

/** `8 min` — `reading-time` says "8 min read", which is a word too many next
 * to a date in a mono meta strip. */
export function shortReadingTime(value: string): string {
  return value.replace(/\s*read$/i, '');
}
