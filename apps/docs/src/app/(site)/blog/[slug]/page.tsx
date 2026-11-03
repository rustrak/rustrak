import Link from 'next/link';
import { notFound } from 'next/navigation';
import { importPage } from 'nextra/pages';
import { MermaidFit } from '@/components/blog/mermaid-fit';
import { PostNeighbours } from '@/components/blog/post-neighbours';
import { ReadingRail, type TocItem } from '@/components/blog/reading-rail';
import { Shell } from '@/components/editorial/shell';
import { Reveal, Words } from '@/components/home/reveal';
import {
  formatDate,
  getNeighbourPosts,
  getPost,
  getPosts,
  shortReadingTime,
} from '@/lib/blog';

type PageProps = {
  params: Promise<{ slug: string }>;
};

export const dynamicParams = false;

export async function generateStaticParams() {
  return getPosts().map((p) => ({ slug: p.slug }));
}

export async function generateMetadata({ params }: PageProps) {
  const { slug } = await params;
  const result = getPost(slug);
  if (!result) return {};
  const { post } = result;
  return {
    title: post.title,
    description: post.description,
    openGraph: {
      title: post.title,
      description: post.description,
      type: 'article',
      publishedTime: post.date,
      authors: [post.author],
      ...(post.image ? { images: [{ url: post.image }] } : {}),
    },
    twitter: {
      card: 'summary_large_image',
      title: post.title,
      description: post.description,
    },
  };
}

/** The element the reading progress is measured against. */
const BODY_ID = 'post-body';

/**
 * A post.
 *
 * The content still goes through Nextra: unlike the changelog, posts carry
 * fenced code, and `importPage` is what gives them highlighting, a copy button
 * and — the reason it matters here — the heading ids and `toc` the rail beside
 * the text is built from. There is nothing to gain by rendering them by hand.
 *
 * The layout is one centred column, the landing's type on it, with the
 * contents pinned to its right from `xl` up. Below that only the rail's
 * reading-progress hairline survives, which a phone wants as much as a desktop.
 */
export default async function BlogPostPage({ params }: PageProps) {
  const { slug } = await params;
  const result = getPost(slug);
  if (!result) notFound();

  const { post } = result;
  const { older, newer } = getNeighbourPosts(slug);

  let MDXContent: React.ComponentType;
  let toc: TocItem[] = [];
  try {
    const imported = await importPage(['blog', slug]);
    MDXContent = imported.default;
    // Third-level headings and deeper would turn the rail into an outline of
    // an outline; two levels is what fits beside the text without scrolling.
    toc = imported.toc.filter((item) => item.depth <= 3);
  } catch {
    notFound();
  }

  return (
    <Shell>
      <header className="mx-auto flex w-full max-w-[47rem] flex-col gap-5 px-6 pt-36 sm:pt-44">
        <Reveal y={12}>
          <Link
            href="/blog"
            className="font-mono text-fg-brand text-mono uppercase tracking-wider transition-colors hover:text-fg"
          >
            ← Blog
          </Link>
        </Reveal>
        <Words
          as="h1"
          text={post.title}
          delay={0.1}
          className="text-balance text-fg text-headline"
        />
        {post.description ? (
          <Reveal delay={0.35}>
            <p className="text-pretty text-fg-subtle text-lead">
              {post.description}
            </p>
          </Reveal>
        ) : null}
        <Reveal
          delay={0.45}
          className="mt-3 flex flex-wrap items-center gap-x-3 gap-y-1 border-border-subtle border-b pb-8 text-fg-meta text-meta"
        >
          <span className="text-fg-secondary">{post.author}</span>
          <span aria-hidden>·</span>
          <time dateTime={post.date}>{formatDate(post.date)}</time>
          <span aria-hidden>·</span>
          <span>{shortReadingTime(post.readingTime)}</span>
          {post.tags.length > 0 ? (
            <span className="font-mono text-mono-sm sm:ms-auto">
              {post.tags.join(' · ')}
            </span>
          ) : null}
        </Reveal>
      </header>

      {/* The text in one centred column; the contents pinned to its right on
          a wide screen, where there is room beside it. */}
      <div className="mx-auto grid w-full max-w-7xl px-6 pt-12 pb-20 xl:grid-cols-[minmax(0,1fr)_44rem_minmax(0,1fr)] xl:gap-x-12">
        <article id={BODY_ID} className="blog-prose min-w-0 xl:col-start-2">
          <MDXContent />
          <MermaidFit bodyId={BODY_ID} />
        </article>
        <div className="min-w-0 xl:col-start-3 xl:row-start-1">
          <ReadingRail items={toc} bodyId={BODY_ID} />
        </div>
      </div>
      <PostNeighbours older={older} newer={newer} />
    </Shell>
  );
}
