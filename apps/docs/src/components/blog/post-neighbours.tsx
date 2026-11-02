import Link from 'next/link';
import { formatDate, type Post } from '@/lib/blog';
import { Reveal } from '../home/reveal';

function Neighbour({ post, label }: { post: Post; label: string }) {
  return (
    <Link
      href={`/blog/${post.slug}`}
      className="group flex h-full flex-col gap-3 rounded-2xl border border-border-subtle bg-panel p-6 transition-colors hover:border-border"
    >
      <span className="font-mono text-fg-meta text-mono-sm uppercase tracking-wider">
        {label}
      </span>
      <span className="text-balance text-fg text-section transition-colors group-hover:text-fg-brand">
        {post.title}
      </span>
      <time dateTime={post.date} className="mt-auto text-fg-meta text-meta">
        {formatDate(post.date)}
      </time>
    </Link>
  );
}

/** The posts either side of this one, as two cards under the text. */
export function PostNeighbours({
  older,
  newer,
}: {
  older: Post | null;
  newer: Post | null;
}) {
  if (!older && !newer) return null;

  return (
    <nav
      aria-label="More posts"
      className="mx-auto grid w-full max-w-[47rem] gap-4 px-6 pb-28 sm:grid-cols-2"
    >
      {newer ? (
        <Reveal>
          <Neighbour post={newer} label="Newer" />
        </Reveal>
      ) : null}
      {older ? (
        <Reveal delay={0.08} className={newer ? undefined : 'sm:col-start-2'}>
          <Neighbour post={older} label="Older" />
        </Reveal>
      ) : null}
    </nav>
  );
}
