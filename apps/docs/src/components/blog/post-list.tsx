import { ArrowRightIcon } from '@rustrak/ui';
import Link from 'next/link';
import { formatDate, type Post, shortReadingTime } from '@/lib/blog';
import { Reveal } from '../home/reveal';

/**
 * One post as one row: the date in a narrow column, the title, the summary
 * and the subjects beside it. Rows sit on hairlines, nothing else frames them.
 */
export function PostEntry({ post, index }: { post: Post; index: number }) {
  return (
    <Reveal as="li" delay={Math.min(index, 4) * 0.06}>
      <Link
        href={`/blog/${post.slug}`}
        className="group grid gap-3 border-border-subtle border-t py-10 sm:grid-cols-[11rem_minmax(0,1fr)] sm:gap-10"
      >
        <div className="flex flex-col gap-1 text-fg-meta text-meta">
          <time dateTime={post.date}>{formatDate(post.date)}</time>
          <span>{shortReadingTime(post.readingTime)}</span>
        </div>
        <div className="flex flex-col gap-3">
          <h2 className="flex items-start gap-3 text-balance text-fg text-page-title transition-colors group-hover:text-fg-brand">
            {post.title}
            <ArrowRightIcon
              aria-hidden
              className="mt-1 size-5 shrink-0 -translate-x-1 opacity-0 transition-[opacity,translate] duration-moderate ease-standard group-hover:translate-x-0 group-hover:opacity-100"
            />
          </h2>
          {post.description ? (
            <p className="max-w-[60ch] text-pretty text-fg-subtle text-value">
              {post.description}
            </p>
          ) : null}
          {post.tags.length > 0 ? (
            <p className="font-mono text-fg-meta text-mono-sm">
              {post.tags.join(' · ')}
            </p>
          ) : null}
        </div>
      </Link>
    </Reveal>
  );
}
