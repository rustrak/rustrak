import type { Metadata } from 'next';
import { PostEntry } from '@/components/blog/post-list';
import { PageIntro } from '@/components/editorial/intro';
import { Shell } from '@/components/editorial/shell';
import { getPosts } from '@/lib/blog';

export const metadata: Metadata = {
  title: 'Blog',
  description:
    'Updates, releases, and engineering notes from the Rustrak project.',
  alternates: { canonical: '/blog' },
};

/** The blog index: the landing's section head, then one row per post. */
export default function BlogPage() {
  const posts = getPosts();

  return (
    <Shell>
      <PageIntro
        eyebrow="Blog"
        title="Notes from building Rustrak."
        lead="Release announcements, and the longer write-ups of how a piece of Rustrak works once it is finished."
      />
      <div className="mx-auto w-full max-w-6xl px-6 pt-16 pb-28">
        {posts.length > 0 ? (
          <ol className="border-border-subtle border-b">
            {posts.map((post, index) => (
              <PostEntry key={post.slug} post={post} index={index} />
            ))}
          </ol>
        ) : (
          <p className="text-fg-subtle text-value">
            No posts yet. Check back soon.
          </p>
        )}
      </div>
    </Shell>
  );
}
